//! Browser backend abstraction layer.
//!
//! This module provides traits and types that abstract over different browser
//! backends (Chromium via CDP, Firefox via RDP). The shell and other components
//! use these traits instead of backend-specific code.
//!
//! # Module Structure
//!
//! - `traits` - Core traits: `BrowserLauncher`, `PageSession`
//! - `types` - Common types: `NodeHandle`, `BrowserEvent`, `BackendError`, etc.
//! - `factory` - Factory function to create launchers by backend kind
//!
//! # Usage
//!
//! ```ignore
//! use abrowser::backend::{create_launcher, BackendKind};
//!
//! let mut launcher = create_launcher(BackendKind::Chromium, None)?;
//! launcher.launch().await?;
//! let session = launcher.create_page("https://example.com").await?;
//! let tree = session.get_accessibility_tree().await?;
//! ```

mod factory;
mod traits;
mod types;

// Re-export everything for convenient access
pub use factory::{create_launcher, default_backend};
pub use traits::{BrowserLauncher, PageSession};
pub use types::{
    BackendError, BackendKind, BackendResult, BrowserEvent, Key, KeyModifiers, MediaStatus,
    NodeHandle, NodeHandleInner,
};
