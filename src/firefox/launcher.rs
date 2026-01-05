//! Firefox browser launcher
//!
//! Launches Firefox with remote debugging enabled and creates a temporary profile
//! with the required preferences for accessibility and debugging.

use crate::backend::{BackendError, BackendKind, BackendResult, BrowserLauncher, PageSession};
use async_trait::async_trait;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::Duration;

use super::client::{read_root_actor, RdpClient};
use super::session::FirefoxSession;

/// Firefox browser launcher
pub struct FirefoxLauncher {
    process: Option<Child>,
    profile_dir: Option<PathBuf>,
    rdp_port: u16,
    viewport: Option<(u32, u32)>,
    client: Option<Arc<RdpClient>>,
}

impl FirefoxLauncher {
    /// Create a new Firefox launcher
    pub fn new(viewport: Option<(u32, u32)>) -> Self {
        Self {
            process: None,
            profile_dir: None,
            rdp_port: 6000,
            viewport,
            client: None,
        }
    }

    /// Find the Firefox binary
    fn find_firefox() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            let paths = [
                "/Applications/Firefox.app/Contents/MacOS/firefox",
                "/Applications/Firefox Developer Edition.app/Contents/MacOS/firefox",
                "/Applications/Firefox Nightly.app/Contents/MacOS/firefox",
            ];
            for path in paths {
                let p = PathBuf::from(path);
                if p.exists() {
                    return Some(p);
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            let paths = [
                "/usr/bin/firefox",
                "/usr/bin/firefox-esr",
                "/snap/bin/firefox",
                "/usr/lib/firefox/firefox",
            ];
            for path in paths {
                let p = PathBuf::from(path);
                if p.exists() {
                    return Some(p);
                }
            }
            // Try PATH
            if let Ok(output) = Command::new("which").arg("firefox").output() {
                if output.status.success() {
                    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !path.is_empty() {
                        return Some(PathBuf::from(path));
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            let paths = [
                r"C:\Program Files\Mozilla Firefox\firefox.exe",
                r"C:\Program Files (x86)\Mozilla Firefox\firefox.exe",
            ];
            for path in paths {
                let p = PathBuf::from(path);
                if p.exists() {
                    return Some(p);
                }
            }
        }

        None
    }

    /// Create a temporary profile with required preferences
    fn create_profile(&self) -> BackendResult<PathBuf> {
        let profile_dir = std::env::temp_dir().join(format!("abrowser-firefox-{}", std::process::id()));
        fs::create_dir_all(&profile_dir)
            .map_err(|e| BackendError::LaunchFailed(format!("Failed to create profile dir: {}", e)))?;

        // Write user.js with required preferences
        let prefs = r#"
// Enable remote debugging
user_pref("devtools.debugger.remote-enabled", true);
user_pref("devtools.chrome.enabled", true);
user_pref("devtools.debugger.prompt-connection", false);

// Enable accessibility - force it on
user_pref("devtools.accessibility.enabled", true);
user_pref("accessibility.force_disabled", -1);  // -1 = force enabled
user_pref("accessibility.AOM.enabled", true);   // Accessibility Object Model

// Allow remote connections from localhost only
user_pref("devtools.debugger.remote-host", "127.0.0.1");

// Disable various prompts and warnings
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.startup.homepage_override.mstone", "ignore");
user_pref("browser.tabs.warnOnClose", false);
user_pref("browser.tabs.warnOnCloseOtherTabs", false);
user_pref("browser.warnOnQuit", false);
user_pref("toolkit.telemetry.reportingpolicy.firstRun", false);
user_pref("datareporting.policy.dataSubmissionEnabled", false);

// Disable updates
user_pref("app.update.enabled", false);
user_pref("app.update.auto", false);

// Performance optimizations for automation
user_pref("browser.sessionstore.resume_from_crash", false);
user_pref("browser.cache.disk.enable", false);
user_pref("browser.cache.memory.enable", true);

// Disable first-run experience
user_pref("browser.aboutwelcome.enabled", false);
user_pref("browser.newtabpage.enabled", false);
user_pref("browser.startup.homepage", "about:blank");
"#;

        let prefs_path = profile_dir.join("user.js");
        let mut file = fs::File::create(&prefs_path)
            .map_err(|e| BackendError::LaunchFailed(format!("Failed to create prefs file: {}", e)))?;
        file.write_all(prefs.as_bytes())
            .map_err(|e| BackendError::LaunchFailed(format!("Failed to write prefs: {}", e)))?;

        Ok(profile_dir)
    }

    /// Wait for Firefox to start accepting connections
    fn wait_for_connection(&self, timeout_secs: u64) -> BackendResult<RdpClient> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_secs);

        while start.elapsed() < timeout {
            match RdpClient::connect(self.rdp_port) {
                Ok(client) => {
                    // Read the root actor greeting
                    match read_root_actor(&client) {
                        Ok(_) => return Ok(client),
                        Err(e) => {
                            crate::utils::log::log(&format!("Failed to read root actor: {}", e));
                            std::thread::sleep(Duration::from_millis(200));
                        }
                    }
                }
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }

        Err(BackendError::LaunchFailed(
            "Timeout waiting for Firefox to start".to_string(),
        ))
    }
}

#[async_trait]
impl BrowserLauncher for FirefoxLauncher {
    fn kind(&self) -> BackendKind {
        BackendKind::Firefox
    }

    async fn launch(&mut self) -> BackendResult<()> {
        if self.process.is_some() {
            return Ok(()); // Already launched
        }

        let firefox_path = Self::find_firefox()
            .ok_or_else(|| BackendError::LaunchFailed("Firefox not found".to_string()))?;

        let profile_dir = self.create_profile()?;
        self.profile_dir = Some(profile_dir.clone());

        // Build command
        let mut cmd = Command::new(&firefox_path);
        cmd.arg("--profile")
            .arg(&profile_dir)
            .arg("--start-debugger-server")
            .arg(self.rdp_port.to_string())
            .arg("--headless")
            .arg("--no-remote");

        // Set viewport if specified
        if let Some((width, height)) = self.viewport {
            cmd.arg("--width").arg(width.to_string());
            cmd.arg("--height").arg(height.to_string());
        }

        // Start about:blank
        cmd.arg("about:blank");

        let child = cmd
            .spawn()
            .map_err(|e| BackendError::LaunchFailed(format!("Failed to start Firefox: {}", e)))?;

        self.process = Some(child);

        // Wait for Firefox to be ready
        let client = self.wait_for_connection(30)?;
        self.client = Some(Arc::new(client));

        Ok(())
    }

    async fn create_page(&self, url: &str) -> BackendResult<Box<dyn PageSession>> {
        let client = self.client.clone()
            .ok_or_else(|| BackendError::NotConnected)?;

        // Create a Firefox session with shared client
        let session = FirefoxSession::new(client, url, self.viewport).await?;

        Ok(Box::new(session))
    }

    fn set_viewport(&mut self, width: u32, height: u32) {
        self.viewport = Some((width, height));
    }

    async fn shutdown(&mut self) -> BackendResult<()> {
        // Drop the client first
        self.client = None;

        // Kill the process
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }

        // Clean up profile directory
        if let Some(profile_dir) = self.profile_dir.take() {
            let _ = fs::remove_dir_all(profile_dir);
        }

        Ok(())
    }

    fn is_running(&self) -> bool {
        self.process.is_some()
    }
}

impl Drop for FirefoxLauncher {
    fn drop(&mut self) {
        // Clean up on drop
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
        if let Some(profile_dir) = self.profile_dir.take() {
            let _ = fs::remove_dir_all(profile_dir);
        }
    }
}
