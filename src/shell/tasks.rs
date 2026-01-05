//! Background task system for async operations

use tokio::task::JoinHandle;
use crate::accessibility::AXTree;

/// Result of a background refresh operation
pub struct RefreshResult {
    pub tree: AXTree,
    pub title: Option<String>,
}

/// A pending background operation that can be polled
pub struct PendingRefresh {
    handle: Option<JoinHandle<Option<RefreshResult>>>,
}

impl PendingRefresh {
    pub fn new() -> Self {
        Self { handle: None }
    }

    /// Check if a refresh is in progress
    pub fn is_pending(&self) -> bool {
        self.handle.as_ref().map(|h| !h.is_finished()).unwrap_or(false)
    }

    /// Start a new refresh (cancels any existing one)
    pub fn start(&mut self, handle: JoinHandle<Option<RefreshResult>>) {
        // Abort any existing task
        if let Some(old) = self.handle.take() {
            old.abort();
        }
        self.handle = Some(handle);
    }

    /// Poll for completed result (non-blocking)
    /// Returns Some(result) if refresh completed, None if still pending or no refresh
    pub fn poll(&mut self) -> Option<RefreshResult> {
        let handle = self.handle.as_ref()?;
        if !handle.is_finished() {
            return None;
        }

        // Take the handle and get result
        let handle = self.handle.take()?;

        // Use now_or_never since we know it's finished
        match futures_util::FutureExt::now_or_never(handle) {
            Some(Ok(Some(result))) => Some(result),
            _ => None,
        }
    }
}

impl Default for PendingRefresh {
    fn default() -> Self {
        Self::new()
    }
}
