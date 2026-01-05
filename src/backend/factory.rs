//! Factory for creating browser launchers.

use super::traits::BrowserLauncher;
use super::types::{BackendKind, BackendResult};
use crate::chromium::ChromiumLauncher;
use crate::firefox::FirefoxLauncher;

/// Create a browser launcher for the specified backend.
///
/// # Arguments
/// * `kind` - The backend to use (Chromium or Firefox)
/// * `viewport` - Optional viewport dimensions (width, height)
///
/// # Returns
/// A boxed BrowserLauncher trait object
pub fn create_launcher(
    kind: BackendKind,
    viewport: Option<(u32, u32)>,
) -> BackendResult<Box<dyn BrowserLauncher>> {
    match kind {
        BackendKind::Chromium => {
            let mut launcher = ChromiumLauncher::new();
            if let Some((w, h)) = viewport {
                launcher = launcher.with_viewport(w, h);
            }
            Ok(Box::new(launcher))
        }
        BackendKind::Firefox => {
            let launcher = FirefoxLauncher::new(viewport);
            Ok(Box::new(launcher))
        }
    }
}

/// Get the default backend kind.
///
/// Currently defaults to Chromium as it's the original implementation.
pub fn default_backend() -> BackendKind {
    BackendKind::Chromium
}
