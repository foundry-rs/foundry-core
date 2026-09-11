import "./styles/App.css";

import { Provider } from "accounts";
import { KeyAuthorization } from "ox/tempo";
import { useCallback, useEffect, useRef, useState } from "react";
import { type Address, type Chain, createWalletClient, custom, type Hex } from "viem";
import { waitForTransactionReceipt } from "viem/actions";

import {
  api,
  applyChainId,
  isOk,
  parseChainId,
  renderJSON,
  renderMaybeParsedJSON,
} from "./utils/helpers.ts";
import { prepareTransactionRequest } from "./utils/transaction.ts";
import type {
  ApiErr,
  ApiOk,
  EIP1193,
  EIP6963AnnounceProviderEvent,
  EIP6963ProviderInfo,
  HistoryEntry,
  KeyAuthorization as KeyAuthorizationDto,
  KeyAuthorizationHistoryEntry,
  PendingAny,
  PendingChainSwitch,
  PendingKeyAuthorization,
  PendingSigning,
  SessionInfo,
  SignHistoryEntry,
  TxHistoryEntry,
} from "./utils/types.ts";

const POLL_REQUEST_INTERVAL_MS = 1000;
const POLL_SESSION_INTERVAL_MS = 3000;
const CHAIN_SWITCH_EVENT_GRACE_MS = 5000;

export function App() {
  useEffect(() => {
    if (!window.__ACCOUNTS_PROVIDER__) {
      window.__ACCOUNTS_PROVIDER__ = Provider.create();
    }
  }, []);

  const [providers, setProviders] = useState<{ info: EIP6963ProviderInfo; provider: EIP1193 }[]>(
    [],
  );
  const [selectedUuid, setSelectedUuid] = useState<string | null>(null);
  const selected = providers.find((p) => p.info.uuid === selectedUuid) ?? null;

  const [account, setAccount] = useState<Address>();
  const [chainId, setChainId] = useState<number>();
  const [chain, setChain] = useState<Chain>();
  const [confirmed, setConfirmed] = useState<boolean>(false);

  const [pendingTx, setPendingTx] = useState<PendingAny | null>(null);
  const [pendingChainSwitch, setPendingChainSwitch] = useState<PendingChainSwitch | null>(null);
  const [pendingSigning, setPendingSigning] = useState<PendingSigning | null>(null);
  const [pendingKeyAuthorization, setPendingKeyAuthorization] =
    useState<PendingKeyAuthorization | null>(null);
  const [isSending, setIsSending] = useState<boolean>(false);

  // In-session history of every signed transaction, message, and key
  // authorization. Flushed on page reload.
  const [history, setHistory] = useState<HistoryEntry[]>([]);

  // Tracks whether the BrowserSigner server is still alive.
  const [sessionAlive, setSessionAlive] = useState<boolean>(true);

  const prevSelectedUuidRef = useRef<string | null>(null);
  const expectedChainSwitchRef = useRef<number | null>(null);
  const recentChainSwitchRef = useRef<{ chainId: number; until: number } | null>(null);

  // --- helpers ---------------------------------------------------------------

  const upsertHistory = useCallback((entry: HistoryEntry) => {
    setHistory((prev) => {
      const idx = prev.findIndex((e) => e.id === entry.id);
      if (idx === -1) {
        return [entry, ...prev];
      }
      const next = prev.slice();
      next[idx] = entry;
      return next;
    });
  }, []);

  const updateHistory = useCallback(
    <K extends HistoryEntry["kind"]>(
      id: string,
      kind: K,
      patch: Partial<Extract<HistoryEntry, { kind: K }>>,
    ) => {
      setHistory((prev) =>
        prev.map((e) =>
          e.id === id && e.kind === kind ? ({ ...e, ...patch } as HistoryEntry) : e,
        ),
      );
    },
    [],
  );

  // --- wallet connection -----------------------------------------------------

  const connect = async () => {
    if (!selected || confirmed) return;

    const addrs = (await selected.provider.request({
      method: "eth_requestAccounts",
    })) as string[];
    setAccount((addrs?.[0] as Address) ?? undefined);

    try {
      const raw = await selected.provider.request<string>({ method: "eth_chainId" });
      applyChainId(raw, setChainId, setChain);
    } catch {
      setChainId(undefined);
      setChain(undefined);
    }
  };

  // Confirm the current connection. After this the polling loop runs and
  // the user no longer needs to reload between requests.
  const confirm = async () => {
    if (!account || chainId == null) return;

    try {
      await api("/api/connection", "POST", { address: account, chainId });
    } catch {
      return;
    }

    setConfirmed(true);
  };

  // Disconnect the wallet. The Rust server will fail any in-flight request
  // with a "Wallet disconnected" error so the script gets fast feedback
  // instead of waiting for the per-request timeout.
  const disconnect = useCallback(async () => {
    try {
      await api("/api/connection", "POST", null);
    } catch {}

    setPendingTx(null);
    setPendingChainSwitch(null);
    setPendingSigning(null);
    setPendingKeyAuthorization(null);
    setAccount(undefined);
    setChainId(undefined);
    setChain(undefined);
    setConfirmed(false);
  }, []);

  // --- request handlers ------------------------------------------------------

  const switchCurrentChain = useCallback(async () => {
    if (!selected || !pendingChainSwitch || !sessionAlive || isSending) return;
    setIsSending(true);

    const { id, chainId: targetChainId } = pendingChainSwitch;
    expectedChainSwitchRef.current = targetChainId;

    let switchedChainId: number | undefined;

    try {
      try {
        await selected.provider.request({
          method: "wallet_switchEthereumChain",
          params: [{ chainId: `0x${targetChainId.toString(16)}` }],
        });

        const raw = await selected.provider.request<string>({ method: "eth_chainId" });
        switchedChainId = parseChainId(raw);
        if (switchedChainId == null) {
          throw new Error(`Wallet returned invalid chain ID ${String(raw)}`);
        }
        if (switchedChainId !== targetChainId) {
          throw new Error(
            `Wallet switched to chain ID ${switchedChainId}, expected ${targetChainId}`,
          );
        }

        applyChainId(raw, setChainId, setChain);
        recentChainSwitchRef.current = {
          chainId: switchedChainId,
          until: Date.now() + CHAIN_SWITCH_EVENT_GRACE_MS,
        };
      } catch (e: unknown) {
        const msg = errMessage(e);
        try {
          await api("/api/chain/response", "POST", { id, chainId: null, error: msg });
        } catch {}
        return;
      }

      try {
        await api("/api/chain/response", "POST", {
          id,
          chainId: switchedChainId,
          error: null,
        });
      } catch (e: unknown) {
        // Wallet is already on the target chain. Do not convert a response
        // reporting failure into a chain-switch failure.
        console.warn("chain switch response post failed after successful switch:", errMessage(e));
      }
    } finally {
      expectedChainSwitchRef.current = null;
      setPendingChainSwitch(null);
      setIsSending(false);
    }
  }, [isSending, pendingChainSwitch, selected, sessionAlive]);

  // Sign and send the current pending transaction. Clears `pendingTx`
  // immediately after the wallet returns a hash so the poller can pick up
  // the next request without waiting for the receipt.
  const signAndSendCurrentTx = async () => {
    if (!selected || !pendingTx?.request || !sessionAlive || isSending) return;
    setIsSending(true);

    const id = pendingTx.id;
    const reqRecord = pendingTx.request as Record<string, unknown>;

    // Push a placeholder history entry immediately so the user sees the
    // request being processed even if signing takes a while.
    const placeholder: TxHistoryEntry = {
      kind: "tx",
      id,
      ts: Date.now(),
      request: reqRecord,
      status: "pending",
    };
    upsertHistory(placeholder);

    const walletClient = createWalletClient({
      transport: custom(selected.provider),
      chain,
    });

    let hash: Hex | undefined;

    try {
      const request = prepareTransactionRequest(reqRecord);
      hash = await walletClient.sendTransaction({
        ...request,
        account: request.account || (await walletClient.getAddresses())[0],
        chain,
      });
    } catch (e: unknown) {
      const msg = errMessage(e);
      console.error("send failed:", msg);

      if (!hash) {
        // Wallet rejected or failed before broadcasting — safe to report error.
        try {
          await api("/api/transaction/response", "POST", { id, hash: null, error: msg });
        } catch {}
        updateHistory(id, "tx", { status: "failed", error: msg });
      } else {
        // Tx was broadcast (hash known) but the response POST failed. Report
        // success so the script is not left waiting; the hash is preserved in
        // history.
        console.warn("response post failed after broadcast, reporting hash anyway:", hash);
        try {
          await api("/api/transaction/response", "POST", { id, hash, error: null });
        } catch {}
        updateHistory(id, "tx", { status: "sent", hash });
      }
      setPendingTx(null);
      setIsSending(false);
      return;
    }

    try {
      await api("/api/transaction/response", "POST", { id, hash, error: null });
    } catch (e: unknown) {
      // Tx is already live on-chain. Log and continue — history already shows
      // the hash.
      console.warn("response post failed after successful send:", errMessage(e));
    }

    // Mark as sent and clear the pending slot so the poller can re-arm
    // immediately for the next request.
    updateHistory(id, "tx", { status: "sent", hash });
    setPendingTx(null);
    setIsSending(false);

    // Fetch the receipt in the background; it'll update the entry when
    // ready. Failures here only affect the displayed receipt, never the
    // signing flow.
    void (async () => {
      try {
        const receipt = await waitForTransactionReceipt(walletClient, { hash: hash as Hex });
        updateHistory(id, "tx", { status: "mined", receipt });
      } catch (e) {
        const msg = errMessage(e);
        updateHistory(id, "tx", { status: "failed", error: msg });
      }
    })();
  };

  // Sign the current pending message / typed data.
  const signCurrentMessage = async () => {
    if (!selected || !pendingSigning || !sessionAlive || isSending) return;
    setIsSending(true);

    const { id, signType, request } = pendingSigning;
    const signer = request.address;
    const msg = request.message;

    const placeholder: SignHistoryEntry = {
      kind: "sign",
      id,
      ts: Date.now(),
      signType,
      request,
      status: "pending",
    };
    upsertHistory(placeholder);

    try {
      let signature: Hex;
      switch (signType) {
        case "PersonalSign":
          signature = (await selected.provider.request({
            method: "personal_sign",
            params: [msg, signer],
          })) as Hex;
          break;
        case "SignTypedDataV4":
          signature = (await selected.provider.request({
            method: "eth_signTypedData_v4",
            params: [signer, msg],
          })) as Hex;
          break;
        default:
          throw new Error(`Unsupported signType: ${signType}`);
      }

      await api("/api/signing/response", "POST", { id, signature, error: null });

      updateHistory(id, "sign", { status: "signed", signature });
    } catch (e: unknown) {
      const msg = errMessage(e);

      try {
        await api("/api/signing/response", "POST", { id, signature: null, error: msg });
      } catch {}

      updateHistory(id, "sign", { status: "failed", error: msg });
    } finally {
      setPendingSigning(null);
      setIsSending(false);
    }
  };

  // Sign the current pending Tempo `KeyAuthorization`. Drives
  // `wallet_authorizeAccessKey` on the connected wallet, then RLP-encodes
  // the returned `keyAuthorization` so the Foundry server can decode it as
  // a `SignedKeyAuthorization`.
  const signCurrentKeyAuthorization = async () => {
    if (!selected || !pendingKeyAuthorization || !sessionAlive || isSending) return;
    setIsSending(true);

    const { id, keyAuthorization: auth, rootAccount } = pendingKeyAuthorization;

    const placeholder: KeyAuthorizationHistoryEntry = {
      kind: "key-authorization",
      id,
      ts: Date.now(),
      keyAuthorization: auth,
      rootAccount,
      status: "pending",
    };
    upsertHistory(placeholder);

    try {
      // Connected wallet must be the root account.
      if (account && account.toLowerCase() !== rootAccount.toLowerCase()) {
        throw new Error(
          `Connected wallet ${account} does not match requested root account ${rootAccount}`,
        );
      }

      const params = keyAuthorizationToWalletParams(auth);

      type WalletKeyAuthorizationRpc = Parameters<typeof KeyAuthorization.fromRpc>[0];
      const resp = (await selected.provider.request({
        method: "wallet_authorizeAccessKey",
        params: [params],
      })) as { keyAuthorization: WalletKeyAuthorizationRpc; rootAddress: `0x${string}` };

      // Convert the wallet's RPC-form keyAuthorization back to the canonical
      // RLP encoding expected by Foundry's BrowserKeyAuthorizationRequest handler.
      const decoded = KeyAuthorization.fromRpc(resp.keyAuthorization);

      // Verify the returned root address matches what the server requested.
      if (resp.rootAddress.toLowerCase() !== rootAccount.toLowerCase()) {
        throw new Error(`Wallet authorized with root ${resp.rootAddress}, expected ${rootAccount}`);
      }

      // Verify the wallet signed the same payload the server queued.
      const actualDigest = KeyAuthorization.hash(decoded);
      if (actualDigest.toLowerCase() !== pendingKeyAuthorization.digest.toLowerCase()) {
        throw new Error(
          `KeyAuthorization digest mismatch: expected ${pendingKeyAuthorization.digest}, got ${actualDigest}`,
        );
      }

      const signedHex = KeyAuthorization.serialize(decoded) as Hex;

      const result = await api<ApiOk<null> | ApiErr>("/api/key-authorization/response", "POST", {
        id,
        signedHex,
        error: null,
      });

      if (isOk(result)) {
        updateHistory(id, "key-authorization", { status: "authorized", signedHex });
      }
    } catch (e: unknown) {
      const msg = errMessage(e);
      console.error("key authorization failed:", msg);

      try {
        await api("/api/key-authorization/response", "POST", { id, signedHex: null, error: msg });
      } catch {}

      updateHistory(id, "key-authorization", { status: "failed", error: msg });
    } finally {
      setPendingKeyAuthorization(null);
      setIsSending(false);
    }
  };

  // Reject the currently pending transaction, signing, or key-authorization
  // request without touching the wallet. Keeps the session alive.
  const rejectCurrent = useCallback(async () => {
    if (isSending) return;
    if (pendingTx) {
      const id = pendingTx.id;
      const reason = "Rejected by user";
      try {
        await api("/api/transaction/response", "POST", { id, hash: null, error: reason });
      } catch {}
      upsertHistory({
        kind: "tx",
        id,
        ts: Date.now(),
        request: pendingTx.request as Record<string, unknown>,
        status: "failed",
        error: reason,
      });
      setPendingTx(null);
      return;
    }
    if (pendingSigning) {
      const { id, signType, request } = pendingSigning;
      const reason = "Rejected by user";
      try {
        await api("/api/signing/response", "POST", { id, signature: null, error: reason });
      } catch {}
      upsertHistory({
        kind: "sign",
        id,
        ts: Date.now(),
        signType,
        request,
        status: "failed",
        error: reason,
      });
      setPendingSigning(null);
      return;
    }
    if (pendingKeyAuthorization) {
      const { id, keyAuthorization, rootAccount } = pendingKeyAuthorization;
      const reason = "Rejected by user";
      try {
        await api("/api/key-authorization/response", "POST", {
          id,
          signedHex: null,
          error: reason,
        });
      } catch {}
      upsertHistory({
        kind: "key-authorization",
        id,
        ts: Date.now(),
        keyAuthorization,
        rootAccount,
        status: "failed",
        error: reason,
      });
      setPendingKeyAuthorization(null);
    }
  }, [isSending, pendingTx, pendingSigning, pendingKeyAuthorization, upsertHistory]);

  // --- effects ---------------------------------------------------------------

  // Reset client state when switching wallets.
  useEffect(() => {
    if (
      prevSelectedUuidRef.current &&
      selectedUuid &&
      prevSelectedUuidRef.current !== selectedUuid
    ) {
      void disconnect();
    }
    prevSelectedUuidRef.current = selectedUuid;
  }, [selectedUuid, disconnect]);

  // Auto-select if only one wallet is available.
  useEffect(() => {
    if (providers.length === 1 && !selected) {
      setSelectedUuid(providers[0].info.uuid);
    }
  }, [providers, selected]);

  // Listen for new provider announcements.
  useEffect(() => {
    const onAnnounce = (ev: EIP6963AnnounceProviderEvent) => {
      const { info, provider } = ev.detail;
      setProviders((prev) =>
        prev.some((p) => p.info.uuid === info.uuid) ? prev : [...prev, { info, provider }],
      );
    };
    window.addEventListener("eip6963:announceProvider", onAnnounce);
    window.dispatchEvent(new Event("eip6963:requestProvider"));
    return () => window.removeEventListener("eip6963:announceProvider", onAnnounce);
  }, []);

  // Listen for account and chain changes.
  useEffect(() => {
    if (!selected) return;

    const isRecentExpectedChainSwitch = (nextChainId?: number) => {
      const recent = recentChainSwitchRef.current;
      if (!recent) return false;
      if (Date.now() > recent.until) {
        recentChainSwitchRef.current = null;
        return false;
      }
      return nextChainId == null || nextChainId === recent.chainId;
    };

    const onAccountsChanged = (accounts: readonly string[]) => {
      const nextAccount = (accounts[0] as Address) ?? undefined;
      if (confirmed) {
        if (
          !nextAccount &&
          (expectedChainSwitchRef.current != null || isRecentExpectedChainSwitch())
        ) {
          return;
        }
        if (nextAccount && account && nextAccount.toLowerCase() === account.toLowerCase()) {
          setAccount(nextAccount);
          return;
        }
        void disconnect();
        return;
      }
      setAccount(nextAccount);
    };
    const onChainChanged = (raw: unknown) => {
      if (confirmed) {
        const nextChainId = parseChainId(raw);
        if (
          nextChainId != null &&
          (nextChainId === expectedChainSwitchRef.current ||
            isRecentExpectedChainSwitch(nextChainId))
        ) {
          applyChainId(raw, setChainId, setChain);
          return;
        }
        void disconnect();
        return;
      }
      applyChainId(raw, setChainId, setChain);
    };

    selected.provider.on?.("accountsChanged", onAccountsChanged);
    selected.provider.on?.("chainChanged", onChainChanged);
    return () => {
      selected.provider.removeListener?.("accountsChanged", onAccountsChanged);
      selected.provider.removeListener?.("chainChanged", onChainChanged);
    };
  }, [selected, confirmed, account, disconnect]);

  useEffect(() => {
    if (!pendingChainSwitch || isSending) return;
    void switchCurrentChain();
  }, [pendingChainSwitch, isSending, switchCurrentChain]);

  // Combined poller: while the session is alive, we are confirmed, and there
  // is no in-flight request, look for the next transaction, signing, or
  // key-authorization request. Polling re-arms automatically as soon as the
  // pending state is cleared by the completion handlers.
  useEffect(() => {
    if (
      !confirmed ||
      pendingTx ||
      pendingChainSwitch ||
      pendingSigning ||
      pendingKeyAuthorization ||
      !sessionAlive
    )
      return;

    let active = true;
    const id = window.setInterval(async () => {
      if (!active) return;

      try {
        const chain = await api<ApiOk<PendingChainSwitch> | ApiErr>("/api/chain/request");
        if (isOk(chain)) {
          if (active) setPendingChainSwitch(chain.data);
          return;
        }
      } catch {}

      try {
        const tx = await api<ApiOk<PendingAny> | ApiErr>("/api/transaction/request");
        if (isOk(tx)) {
          if (active) setPendingTx(tx.data);
          return;
        }
      } catch {}

      try {
        const sig = await api<ApiOk<PendingSigning> | ApiErr>("/api/signing/request");
        if (isOk(sig)) {
          if (active) setPendingSigning(sig.data);
          return;
        }
      } catch {}

      try {
        const auth = await api<ApiOk<PendingKeyAuthorization> | ApiErr>(
          "/api/key-authorization/request",
        );
        if (isOk(auth)) {
          if (active) setPendingKeyAuthorization(auth.data);
        }
      } catch {}
    }, POLL_REQUEST_INTERVAL_MS);

    return () => {
      active = false;
      window.clearInterval(id);
    };
  }, [
    confirmed,
    pendingTx,
    pendingChainSwitch,
    pendingSigning,
    pendingKeyAuthorization,
    sessionAlive,
  ]);

  // Session liveness poller: detect when the BrowserSigner server is gone
  // (script finished, server stopped) so we can stop spamming the request
  // endpoints and surface a clean "session ended" UI.
  useEffect(() => {
    let active = true;
    const tick = async () => {
      try {
        const resp = await api<ApiOk<SessionInfo> | ApiErr>("/api/session");
        if (!active) return;
        if (isOk(resp)) {
          setSessionAlive(resp.data.alive);
        } else {
          setSessionAlive(false);
        }
      } catch {
        if (active) setSessionAlive(false);
      }
    };
    void tick();
    const id = window.setInterval(tick, POLL_SESSION_INTERVAL_MS);
    return () => {
      active = false;
      window.clearInterval(id);
    };
  }, []);

  // --- render ---------------------------------------------------------------

  return (
    <div className="wrapper">
      <div className="container">
        <div className="notice">
          Browser wallet is still in early development. Use with caution!
        </div>

        <img className="banner" src="banner.png" alt="Foundry Browser Wallet" />

        {!sessionAlive && (
          <div className="session-ended">
            Session ended. The script has finished or the server is no longer reachable.
          </div>
        )}

        {providers.length > 1 && sessionAlive && (
          <div className="wallet-selector">
            <label>
              <select
                value={selectedUuid ?? ""}
                onChange={(e) => setSelectedUuid(e.target.value || null)}
                disabled={confirmed}
              >
                <option value="" disabled>
                  Select wallet…
                </option>
                {providers.map(({ info }) => (
                  <option key={info.uuid} value={info.uuid}>
                    {info.name} ({info.rdns})
                  </option>
                ))}
              </select>
            </label>
          </div>
        )}

        {providers.length === 0 && sessionAlive && <p>No wallets found.</p>}

        {selected && !account && sessionAlive && (
          <button
            type="button"
            className="btn btn-primary wallet-connect"
            onClick={connect}
            disabled={confirmed}
          >
            Connect Wallet
          </button>
        )}

        {selected && account && !confirmed && (
          <button
            type="button"
            className="btn btn-primary wallet-confirm"
            onClick={confirm}
            disabled={!account || chainId == null}
          >
            Confirm Connection
          </button>
        )}

        {selected && account && confirmed && (
          <>
            <div className="section-title">Connected</div>
            <pre className="box">
              {`\
account: ${account}
chain:   ${chain ? `${chain.name} (${chainId})` : (chainId ?? "unknown")}
rpc:     ${chain?.rpcUrls?.default?.http?.[0] ?? chain?.rpcUrls?.public?.http?.[0] ?? "unknown"}`}
            </pre>
            <div className="disconnect-row">
              <button type="button" className="btn btn-secondary" onClick={() => void disconnect()}>
                Disconnect
              </button>
            </div>
          </>
        )}

        {selected && account && confirmed && sessionAlive && pendingTx && (
          <>
            <div className="section-title">Transaction to Sign &amp; Send</div>
            <div className="action-row">
              <button
                type="button"
                className="btn btn-primary"
                onClick={signAndSendCurrentTx}
                disabled={isSending || !sessionAlive}
              >
                Sign &amp; Send
              </button>
              <button
                type="button"
                className="btn btn-danger"
                onClick={() => void rejectCurrent()}
                disabled={isSending || !sessionAlive}
              >
                Reject
              </button>
            </div>
            <div className="box">
              <pre>{renderJSON(pendingTx.request)}</pre>
            </div>
          </>
        )}

        {selected && account && confirmed && sessionAlive && !pendingTx && pendingSigning && (
          <>
            <div className="section-title">Message / Data to Sign</div>
            <div className="action-row">
              <button
                type="button"
                className="btn btn-primary"
                onClick={signCurrentMessage}
                disabled={isSending || !sessionAlive}
              >
                Sign
              </button>
              <button
                type="button"
                className="btn btn-danger"
                onClick={() => void rejectCurrent()}
                disabled={isSending || !sessionAlive}
              >
                Reject
              </button>
            </div>
            <div className="box">
              <pre>{renderMaybeParsedJSON(pendingSigning.request)}</pre>
            </div>
          </>
        )}

        {selected &&
          account &&
          confirmed &&
          sessionAlive &&
          !pendingTx &&
          !pendingSigning &&
          pendingKeyAuthorization && (
            <>
              <div className="section-title">Authorize Access Key</div>
              <div className="action-row">
                <button
                  type="button"
                  className="btn btn-primary"
                  onClick={signCurrentKeyAuthorization}
                  disabled={isSending || !sessionAlive}
                >
                  Authorize
                </button>
                <button
                  type="button"
                  className="btn btn-danger"
                  onClick={() => void rejectCurrent()}
                  disabled={isSending || !sessionAlive}
                >
                  Reject
                </button>
              </div>
              <div className="box">
                <pre>{summarizeKeyAuthorization(pendingKeyAuthorization)}</pre>
              </div>
            </>
          )}

        {selected &&
          account &&
          confirmed &&
          !pendingTx &&
          !pendingChainSwitch &&
          !pendingSigning &&
          !pendingKeyAuthorization &&
          history.length === 0 &&
          sessionAlive && (
            <>
              <div className="section-title">Waiting</div>
              <div className="box">
                <pre>No pending transaction, signing, or key authorization request</pre>
              </div>
            </>
          )}

        {history.length > 0 && (
          <>
            <div className="section-title">Session history</div>
            <div className="history">
              {history.map((entry) => (
                <HistoryCard key={entry.id} entry={entry} />
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}

// --- subcomponents ----------------------------------------------------------

function HistoryCard({ entry }: { entry: HistoryEntry }) {
  const [open, setOpen] = useState(false);

  if (entry.kind === "tx") {
    const summary =
      entry.status === "mined"
        ? `mined  ${entry.hash ?? ""}`
        : entry.status === "sent"
          ? `sent   ${entry.hash ?? ""}  (waiting for receipt)`
          : entry.status === "failed"
            ? `failed ${(entry.error ?? "").split("\n")[0]}`
            : "pending";
    return (
      <div className={`history-entry tx status-${entry.status}${open ? " open" : ""}`}>
        <button
          type="button"
          className="history-summary"
          onClick={() => setOpen((o) => !o)}
          aria-expanded={open}
        >
          <span className="history-kind">tx</span>
          <span className="history-status">{summary}</span>
          <Chevron />
        </button>
        {open && (
          <div className="history-details">
            <div className="section-subtitle">Request</div>
            <pre className="box">{renderJSON(entry.request)}</pre>
            {entry.hash && (
              <>
                <div className="section-subtitle">Hash</div>
                <pre className="box">{entry.hash}</pre>
              </>
            )}
            {entry.receipt && (
              <>
                <div className="section-subtitle">Receipt</div>
                <pre className="box">{renderJSON(entry.receipt)}</pre>
              </>
            )}
            {entry.status === "sent" && !entry.receipt && (
              <>
                <div className="section-subtitle">Receipt</div>
                <pre className="box">Waiting for receipt…</pre>
              </>
            )}
            {entry.error && (
              <>
                <div className="section-subtitle">Error</div>
                <pre className="box error">{entry.error}</pre>
              </>
            )}
          </div>
        )}
      </div>
    );
  }

  if (entry.kind === "sign") {
    const summary =
      entry.status === "signed"
        ? `signed ${shortHash(entry.signature)}`
        : entry.status === "failed"
          ? `failed ${(entry.error ?? "").split("\n")[0]}`
          : "pending";
    return (
      <div className={`history-entry sign status-${entry.status}${open ? " open" : ""}`}>
        <button
          type="button"
          className="history-summary"
          onClick={() => setOpen((o) => !o)}
          aria-expanded={open}
        >
          <span className="history-kind">sig</span>
          <span className="history-status">{summary}</span>
          <Chevron />
        </button>
        {open && (
          <div className="history-details">
            <div className="section-subtitle">Type</div>
            <pre className="box">{entry.signType}</pre>
            <div className="section-subtitle">Request</div>
            <pre className="box">{renderMaybeParsedJSON(entry.request)}</pre>
            {entry.signature && (
              <>
                <div className="section-subtitle">Signature</div>
                <pre className="box">{entry.signature}</pre>
              </>
            )}
            {entry.error && (
              <>
                <div className="section-subtitle">Error</div>
                <pre className="box error">{entry.error}</pre>
              </>
            )}
          </div>
        )}
      </div>
    );
  }

  // entry.kind === "key-authorization"
  const summary =
    entry.status === "authorized"
      ? `authorized ${shortHash(entry.signedHex)}`
      : entry.status === "failed"
        ? `failed ${(entry.error ?? "").split("\n")[0]}`
        : "pending";
  return (
    <div className={`history-entry key-authorization status-${entry.status}${open ? " open" : ""}`}>
      <button
        type="button"
        className="history-summary"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
      >
        <span className="history-kind">key</span>
        <span className="history-status">{summary}</span>
        <Chevron />
      </button>
      {open && (
        <div className="history-details">
          <div className="section-subtitle">Key ID</div>
          <pre className="box">{entry.keyAuthorization.keyId}</pre>
          <div className="section-subtitle">Root Account</div>
          <pre className="box">{entry.rootAccount}</pre>
          {entry.signedHex && (
            <>
              <div className="section-subtitle">Signed Authorization</div>
              <pre className="box">{entry.signedHex}</pre>
            </>
          )}
          {entry.error && (
            <>
              <div className="section-subtitle">Error</div>
              <pre className="box error">{entry.error}</pre>
            </>
          )}
        </div>
      )}
    </div>
  );
}

// Chevron icon used in history dropdown summaries. Inherits color from
// its parent and is rotated 180° via CSS when the entry is open.
function Chevron() {
  return (
    <svg
      className="history-chevron"
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <polyline points="6 9 12 15 18 9" />
    </svg>
  );
}

// --- utilities --------------------------------------------------------------

function parseTempoChainId(chainId: `0x${string}`): bigint {
  return chainId === "0x" ? 0n : BigInt(chainId);
}

function hexToSafeNumber(hex: `0x${string}`): number {
  const n = BigInt(hex);
  if (n > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error(`Value ${n} exceeds Number.MAX_SAFE_INTEGER`);
  }
  return Number(n);
}

function shortHash(h?: string): string {
  if (!h) return "";
  return h.length > 18 ? `${h.slice(0, 10)}…${h.slice(-8)}` : h;
}

function errMessage(e: unknown): string {
  return typeof e === "object" &&
    e &&
    "message" in e &&
    typeof (e as { message?: unknown }).message === "string"
    ? (e as { message: string }).message
    : String(e);
}

// Convert a Tempo `KeyAuthorization` (as emitted by Foundry's
// `BrowserKeyAuthorizationRequest`) into the parameter shape required by the
// `wallet_authorizeAccessKey` RPC method (see `accounts/dist/core/zod/rpc`).
function keyAuthorizationToWalletParams(auth: KeyAuthorizationDto): Record<string, unknown> {
  const expiry =
    auth.expiry == null || auth.expiry === "0x" || auth.expiry === "0x0"
      ? 0
      : hexToSafeNumber(auth.expiry as `0x${string}`);

  const limits = auth.limits?.map((l) => ({
    token: l.token,
    limit: BigInt(l.limit),
    ...(l.period ? { period: hexToSafeNumber(l.period as `0x${string}`) } : {}),
  }));

  // Flatten Tempo's nested `allowedCalls` into the SDK's flat `scopes`:
  //   - CallScope without selectorRules -> `{ address: target }` (any selector)
  //   - CallScope with selectorRules    -> one scope per rule
  const scopes = (auth.allowedCalls ?? []).flatMap((cs) => {
    if (!cs.selectorRules || cs.selectorRules.length === 0) {
      return [{ address: cs.target }];
    }
    return cs.selectorRules.map((r) => ({
      address: cs.target,
      selector: r.selector,
      ...(r.recipients && r.recipients.length > 0 ? { recipients: r.recipients } : {}),
    }));
  });

  return {
    address: auth.keyId,
    chainId: parseTempoChainId(auth.chainId),
    expiry,
    keyType: auth.keyType,
    ...(auth.limits != null ? { limits } : {}),
    ...(auth.allowedCalls ? { scopes } : {}),
  };
}

// Render a human-readable summary of a pending Tempo `KeyAuthorization`
// for the approval card.
function summarizeKeyAuthorization(req: PendingKeyAuthorization): string {
  const { keyAuthorization: auth, rootAccount, digest } = req;
  const lines: string[] = [];
  lines.push(`Authorize key: ${auth.keyId}`);
  lines.push(`On account:    ${rootAccount}`);
  lines.push(`Chain ID:      ${parseTempoChainId(auth.chainId).toString()}`);
  lines.push(`Key type:      ${auth.keyType}`);

  if (auth.expiry == null || auth.expiry === "0x" || auth.expiry === "0x0") {
    lines.push("Expiry:        never");
  } else {
    const ts = hexToSafeNumber(auth.expiry as `0x${string}`);
    lines.push(`Expiry:        ${new Date(ts * 1000).toISOString()} (${ts})`);
  }

  if (auth.limits == null) {
    lines.push("Spending:      unlimited");
  } else if (auth.limits.length === 0) {
    lines.push("Spending:      none (deny all)");
  } else {
    lines.push("Spending limits:");
    for (const l of auth.limits) {
      const period = l.period ? ` per ${BigInt(l.period).toString()}s` : "";
      lines.push(`  - ${l.token}: ${BigInt(l.limit).toString()}${period}`);
    }
  }

  if (auth.allowedCalls == null) {
    lines.push("Allowed calls: any");
  } else if (auth.allowedCalls.length === 0) {
    lines.push("Allowed calls: none (deny all)");
  } else {
    lines.push("Allowed calls:");
    for (const cs of auth.allowedCalls) {
      if (!cs.selectorRules || cs.selectorRules.length === 0) {
        lines.push(`  - ${cs.target}: any selector`);
      } else {
        for (const r of cs.selectorRules) {
          const recipients =
            r.recipients && r.recipients.length > 0 ? ` to [${r.recipients.join(", ")}]` : "";
          lines.push(`  - ${cs.target}: ${r.selector}${recipients}`);
        }
      }
    }
  }

  lines.push("");
  lines.push(`Digest: ${digest}`);

  return lines.join("\n");
}
