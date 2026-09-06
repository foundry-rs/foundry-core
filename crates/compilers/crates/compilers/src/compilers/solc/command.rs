//! Retry transient executable-file-busy errors before a compiler process has started.

use std::{io, time::Duration};

// A renamed compiler can still have a writable descriptor inherited by a concurrent fork.
// Use five retries with 310 ms of total backoff so a persistently busy executable still errors.
const RETRY_DELAYS: [Duration; 5] = [
    Duration::from_millis(10),
    Duration::from_millis(20),
    Duration::from_millis(40),
    Duration::from_millis(80),
    Duration::from_millis(160),
];

pub(super) fn retry_spawn<T>(mut spawn: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    for delay in RETRY_DELAYS {
        match spawn() {
            Err(err) if err.kind() == io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(delay);
            }
            result => return result,
        }
    }
    spawn()
}

#[cfg(feature = "async")]
pub(super) async fn async_retry_spawn<T>(
    mut spawn: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    for delay in RETRY_DELAYS {
        match spawn() {
            Err(err) if err.kind() == io::ErrorKind::ExecutableFileBusy => {
                // Do not require a Tokio timer driver or a free blocking-pool thread:
                // callers may be driving this future from a blocking task themselves.
                futures_timer::Delay::new(delay).await;
            }
            result => return result,
        }
    }
    spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_retry_other_errors() {
        for kind in
            [io::ErrorKind::PermissionDenied, io::ErrorKind::NotFound, io::ErrorKind::Interrupted]
        {
            let mut attempts = 0;
            let result: io::Result<()> = retry_spawn(|| {
                attempts += 1;
                Err(io::Error::from(kind))
            });
            assert_eq!(result.unwrap_err().kind(), kind);
            assert_eq!(attempts, 1);
        }
    }

    #[test]
    fn persistent_busy_is_bounded() {
        let mut attempts = 0;
        let result: io::Result<()> = retry_spawn(|| {
            attempts += 1;
            Err(io::Error::from(io::ErrorKind::ExecutableFileBusy))
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::ExecutableFileBusy);
        assert_eq!(attempts, RETRY_DELAYS.len() + 1);
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn async_does_not_retry_other_errors() {
        for kind in
            [io::ErrorKind::PermissionDenied, io::ErrorKind::NotFound, io::ErrorKind::Interrupted]
        {
            let mut attempts = 0;
            let result: io::Result<()> = async_retry_spawn(|| {
                attempts += 1;
                Err(io::Error::from(kind))
            })
            .await;
            assert_eq!(result.unwrap_err().kind(), kind);
            assert_eq!(attempts, 1);
        }
    }

    #[cfg(feature = "async")]
    #[tokio::test]
    async fn async_persistent_busy_is_bounded() {
        let mut attempts = 0;
        let result: io::Result<()> = async_retry_spawn(|| {
            attempts += 1;
            Err(io::Error::from(io::ErrorKind::ExecutableFileBusy))
        })
        .await;
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::ExecutableFileBusy);
        assert_eq!(attempts, RETRY_DELAYS.len() + 1);
    }
}
