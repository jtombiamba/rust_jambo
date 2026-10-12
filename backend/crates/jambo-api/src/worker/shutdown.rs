//! Graceful shutdown coordination for long-running worker tasks.
//!
//! A single [`ShutdownHandle`] is created by the entry point and used to signal
//! every spawned task. Tasks receive cheap, cloneable [`ShutdownSignal`]s they
//! can poll or await.

use tokio::sync::watch;

/// Sender-side handle used to request a graceful shutdown.
#[derive(Debug, Clone)]
pub struct ShutdownHandle {
    tx: watch::Sender<bool>,
}

impl ShutdownHandle {
    /// Creates a new shutdown channel, initially in the "not shutting down" state.
    pub fn new() -> Self {
        let (tx, _rx) = watch::channel(false);
        Self { tx }
    }

    /// Returns a [`ShutdownSignal`] that tracks the shutdown state.
    pub fn signal(&self) -> ShutdownSignal {
        ShutdownSignal {
            rx: self.tx.subscribe(),
        }
    }

    /// Requests shutdown. Idempotent: subsequent calls are no-ops.
    pub fn trigger(&self) {
        // `send_replace` (unlike `send`) always stores the value, even if there
        // are currently no subscribers — important because the shutdown signal
        // may be produced before any task has subscribed to it.
        let _ = self.tx.send_replace(true);
    }
}

impl Default for ShutdownHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Receiver-side shutdown signal.
///
/// Cheap to clone (`watch::Receiver` is `Clone`) and safe to share across
/// spawned tasks.
#[derive(Debug, Clone)]
pub struct ShutdownSignal {
    rx: watch::Receiver<bool>,
}

impl ShutdownSignal {
    /// Returns `true` once shutdown has been requested.
    pub fn is_shutdown(&self) -> bool {
        *self.rx.borrow()
    }

    /// Resolves once shutdown has been requested. Returns immediately if it was
    /// already requested before this call.
    pub async fn wait(&self) {
        let mut rx = self.rx.clone();
        if *rx.borrow_and_update() {
            return;
        }
        let _ = rx.changed().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn signal_defaults_to_not_shutdown() {
        let handle = ShutdownHandle::new();
        assert!(!handle.signal().is_shutdown());
    }

    #[test]
    fn trigger_flips_signal_and_is_idempotent() {
        let handle = ShutdownHandle::new();
        let signal = handle.signal();
        assert!(!signal.is_shutdown());

        handle.trigger();
        assert!(signal.is_shutdown());

        // Triggering again is a no-op and keeps the signal set.
        handle.trigger();
        assert!(signal.is_shutdown());
    }

    #[test]
    fn multiple_signals_see_the_same_state() {
        let handle = ShutdownHandle::new();
        let a = handle.signal();
        let b = handle.signal();
        handle.trigger();
        assert!(a.is_shutdown());
        assert!(b.is_shutdown());
    }

    #[test]
    fn trigger_before_subscribe_is_visible() {
        // Regression: triggering with no live subscribers must still store the
        // shutdown state so a later subscriber observes it.
        let handle = ShutdownHandle::new();
        handle.trigger();
        assert!(handle.signal().is_shutdown());
    }

    #[tokio::test]
    async fn wait_resolves_after_trigger() {
        let handle = ShutdownHandle::new();
        let signal = handle.signal();
        let trigger = handle.clone();

        let waiter = tokio::spawn(async move {
            signal.wait().await;
            true
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!waiter.is_finished());

        trigger.trigger();
        assert!(waiter.await.unwrap());
    }

    #[tokio::test]
    async fn wait_resolves_immediately_if_already_shutdown() {
        let handle = ShutdownHandle::new();
        handle.trigger();
        // Must not hang.
        handle.signal().wait().await;
    }
}
