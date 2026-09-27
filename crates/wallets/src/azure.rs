//! Azure Key Vault credential resolution.

use alloy_signer_azure::{
    azure_core::{
        self,
        credentials::{AccessToken, TokenCredential, TokenRequestOptions},
        error::ErrorKind,
    },
    azure_identity::{
        ClientSecretCredential, DeveloperToolsCredential, ManagedIdentityCredential,
        ManagedIdentityCredentialOptions, UserAssignedId, WorkloadIdentityCredential,
    },
};
use async_trait::async_trait;
use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

/// Maximum time to wait for a managed identity token.
///
/// Outside Azure, the managed identity endpoint may accept connections without answering, and the
/// Azure SDK retries it for over a minute.
const MANAGED_IDENTITY_TIMEOUT: Duration = Duration::from_secs(10);

/// Returns the credential used to authenticate Azure Key Vault requests.
///
/// The credential is selected from the environment once per process, in this order:
/// 1. A service principal secret, when `AZURE_CLIENT_SECRET` is set. Requires `AZURE_TENANT_ID` and
///    `AZURE_CLIENT_ID`.
/// 2. Workload identity, when `AZURE_FEDERATED_TOKEN_FILE` is set. Requires `AZURE_TENANT_ID` and
///    `AZURE_CLIENT_ID`.
/// 3. Otherwise, the Azure CLI or Azure Developer CLI, then a managed identity. Unlike the Azure
///    SDK's default credential, developer tools are tried first so that a local login is not
///    delayed by the managed identity endpoint. The managed identity is user-assigned when
///    `AZURE_CLIENT_ID` is set.
pub(crate) fn credential() -> azure_core::Result<Arc<dyn TokenCredential>> {
    static CREDENTIAL: OnceLock<Arc<dyn TokenCredential>> = OnceLock::new();
    if let Some(credential) = CREDENTIAL.get() {
        return Ok(credential.clone());
    }
    let credential = credential_from_env(|key| std::env::var(key).ok())?;
    Ok(CREDENTIAL.get_or_init(|| credential).clone())
}

/// Returns the credential selected by [`credential`], reading variables through `env`.
fn credential_from_env(
    env: impl Fn(&str) -> Option<String>,
) -> azure_core::Result<Arc<dyn TokenCredential>> {
    let required = |key: &str| {
        env(key).ok_or_else(|| {
            azure_core::Error::with_message(
                ErrorKind::Credential,
                format!("{key} environment variable is required for Azure Key Vault signer"),
            )
        })
    };

    if let Some(secret) = env("AZURE_CLIENT_SECRET") {
        let tenant_id = required("AZURE_TENANT_ID")?;
        let client_id = required("AZURE_CLIENT_ID")?;
        return Ok(ClientSecretCredential::new(&tenant_id, client_id, secret.into(), None)?);
    }
    if env("AZURE_FEDERATED_TOKEN_FILE").is_some() {
        return Ok(WorkloadIdentityCredential::new(None)?);
    }

    let mut sources: Vec<Arc<dyn TokenCredential>> = vec![DeveloperToolsCredential::new(None)?];
    // Managed identity is unsupported in some environments, e.g. Azure Cloud Shell, where the
    // developer tools still work.
    match ManagedIdentityCredential::new(Some(ManagedIdentityCredentialOptions {
        user_assigned_id: env("AZURE_CLIENT_ID").map(UserAssignedId::ClientId),
        ..Default::default()
    })) {
        Ok(managed_identity) => sources.push(Arc::new(TimeoutCredential {
            inner: managed_identity,
            timeout: MANAGED_IDENTITY_TIMEOUT,
        })),
        Err(err) => debug!(%err, "managed identity credential unavailable"),
    }
    Ok(Arc::new(ChainedCredential { sources, selected: AtomicUsize::new(usize::MAX) }))
}

/// Tries each credential source in order and keeps using the first that returns a token.
#[derive(Debug)]
struct ChainedCredential {
    sources: Vec<Arc<dyn TokenCredential>>,
    /// Index of the source that first returned a token, or `usize::MAX` if none did yet.
    selected: AtomicUsize,
}

#[async_trait]
impl TokenCredential for ChainedCredential {
    async fn get_token(
        &self,
        scopes: &[&str],
        options: Option<TokenRequestOptions<'_>>,
    ) -> azure_core::Result<AccessToken> {
        if let Some(source) = self.sources.get(self.selected.load(Ordering::Relaxed)) {
            return source.get_token(scopes, options).await;
        }

        let mut errors = Vec::with_capacity(self.sources.len());
        for (index, source) in self.sources.iter().enumerate() {
            match source.get_token(scopes, options.clone()).await {
                Ok(token) => {
                    self.selected.store(index, Ordering::Relaxed);
                    return Ok(token);
                }
                Err(err) => errors.push(err.to_string()),
            }
        }
        Err(azure_core::Error::with_message(
            ErrorKind::Credential,
            format!("no Azure credential source returned a token:\n{}", errors.join("\n")),
        ))
    }
}

/// Fails a credential source that does not return a token within `timeout`.
#[derive(Debug)]
struct TimeoutCredential {
    inner: Arc<dyn TokenCredential>,
    timeout: Duration,
}

#[async_trait]
impl TokenCredential for TimeoutCredential {
    async fn get_token(
        &self,
        scopes: &[&str],
        options: Option<TokenRequestOptions<'_>>,
    ) -> azure_core::Result<AccessToken> {
        tokio::time::timeout(self.timeout, self.inner.get_token(scopes, options)).await.map_err(
            |_| {
                azure_core::Error::with_message(
                    ErrorKind::Credential,
                    format!("managed identity timed out after {} seconds", self.timeout.as_secs()),
                )
            },
        )?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_signer_azure::azure_core::time::OffsetDateTime;
    use std::{collections::HashMap, sync::atomic::AtomicU32};

    /// A credential source that returns a fixed result, optionally never completing.
    #[derive(Debug, Default)]
    struct MockSource {
        token: Option<&'static str>,
        hang: bool,
        calls: AtomicU32,
    }

    impl MockSource {
        fn token(token: &'static str) -> Arc<Self> {
            Arc::new(Self { token: Some(token), ..Default::default() })
        }

        fn fail() -> Arc<Self> {
            Arc::new(Self::default())
        }

        fn hang() -> Arc<Self> {
            Arc::new(Self { hang: true, ..Default::default() })
        }
    }

    #[async_trait]
    impl TokenCredential for MockSource {
        async fn get_token(
            &self,
            _scopes: &[&str],
            _options: Option<TokenRequestOptions<'_>>,
        ) -> azure_core::Result<AccessToken> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if self.hang {
                std::future::pending::<()>().await;
            }
            let Some(token) = self.token else {
                return Err(azure_core::Error::with_message(ErrorKind::Credential, "no login"));
            };
            Ok(AccessToken::new(token, OffsetDateTime::now_utc()))
        }
    }

    fn chain(sources: Vec<Arc<dyn TokenCredential>>) -> ChainedCredential {
        ChainedCredential { sources, selected: AtomicUsize::new(usize::MAX) }
    }

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let vars = vars
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<HashMap<_, _>>();
        move |key| vars.get(key).cloned()
    }

    #[tokio::test]
    async fn chain_keeps_first_working_source() {
        let (cli, managed) = (MockSource::fail(), MockSource::token("managed"));
        let credential = chain(vec![cli.clone(), managed.clone()]);

        for _ in 0..2 {
            let token = credential.get_token(&["scope"], None).await.unwrap();
            assert_eq!(token.token.secret(), "managed");
        }
        assert_eq!(cli.calls.load(Ordering::Relaxed), 1);
        assert_eq!(managed.calls.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn chain_reports_every_failure() {
        let credential = chain(vec![MockSource::fail(), MockSource::fail()]);
        let err = credential.get_token(&["scope"], None).await.unwrap_err().to_string();
        assert_eq!(err, "no Azure credential source returned a token:\nno login\nno login");
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_fails_hanging_source() {
        let credential =
            TimeoutCredential { inner: MockSource::hang(), timeout: Duration::from_secs(10) };
        let err = credential.get_token(&["scope"], None).await.unwrap_err().to_string();
        assert_eq!(err, "managed identity timed out after 10 seconds");
    }

    #[test]
    fn client_secret_requires_tenant_and_client_id() {
        let err = credential_from_env(env(&[("AZURE_CLIENT_SECRET", "secret")])).unwrap_err();
        assert_eq!(
            err.to_string(),
            "AZURE_TENANT_ID environment variable is required for Azure Key Vault signer"
        );

        let err = credential_from_env(env(&[
            ("AZURE_CLIENT_SECRET", "secret"),
            ("AZURE_TENANT_ID", "00000000-0000-0000-0000-000000000000"),
        ]))
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "AZURE_CLIENT_ID environment variable is required for Azure Key Vault signer"
        );
    }

    #[test]
    fn builds_credential_from_environment() {
        let tenant = ("AZURE_TENANT_ID", "00000000-0000-0000-0000-000000000000");
        let client = ("AZURE_CLIENT_ID", "11111111-1111-1111-1111-111111111111");

        credential_from_env(env(&[("AZURE_CLIENT_SECRET", "secret"), tenant, client])).unwrap();
        credential_from_env(env(&[client])).unwrap();
        credential_from_env(env(&[])).unwrap();
    }
}
