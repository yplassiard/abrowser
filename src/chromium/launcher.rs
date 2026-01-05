//! Chromium browser launcher implementing the BrowserLauncher trait.

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use async_trait::async_trait;

use super::client::CdpClient;
use super::session::ChromiumSession;
use crate::backend::{BackendKind, BackendResult, BackendError, BrowserLauncher, PageSession};

pub struct ChromiumLauncher {
    process: Option<Child>,
    debug_port: u16,
    browser_ws_url: Option<String>,
    browser_client: Option<CdpClient>,
    viewport: (u32, u32),
}

impl ChromiumLauncher {
    pub fn new() -> Self {
        let port = Self::find_available_port().unwrap_or(9222);
        Self {
            process: None,
            debug_port: port,
            browser_ws_url: None,
            browser_client: None,
            viewport: (375, 812), // Default to mobile
        }
    }

    pub fn with_viewport(mut self, width: u32, height: u32) -> Self {
        self.viewport = (width, height);
        self
    }

    /// Find an available port for debugging
    fn find_available_port() -> Option<u16> {
        TcpListener::bind("127.0.0.1:0")
            .ok()
            .and_then(|listener| listener.local_addr().ok())
            .map(|addr| addr.port())
    }

    /// Kill any existing Chromium processes with remote debugging enabled
    fn kill_existing_debug_processes() {
        #[cfg(unix)]
        {
            let _ = Command::new("pkill")
                .args(["-9", "-f", "remote-debugging-port"])
                .output();
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Find the headless_shell binary
    fn find_binary() -> Option<PathBuf> {
        let candidates = [
            // Built from source
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("chromium/src/out/Release/headless_shell"),
            // System Chrome (macOS)
            PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
            // System Chromium (macOS)
            PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
            // Linux - full browsers first (support audio)
            PathBuf::from("/usr/bin/google-chrome"),
            PathBuf::from("/usr/bin/chromium"),
            PathBuf::from("/usr/bin/chromium-browser"),
            // Linux - headless-shell fallback (no audio)
            PathBuf::from("/usr/lib/chromium/headless_shell"),
            PathBuf::from("/usr/bin/chromium-headless-shell"),
        ];

        for path in candidates {
            if path.exists() {
                return Some(path);
            }
        }

        // Check PATH
        if let Ok(output) = Command::new("which").arg("chromium").output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(PathBuf::from(path));
                }
            }
        }

        None
    }

    fn find_ws_url<R: std::io::Read>(
        reader: BufReader<R>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let timeout = Duration::from_secs(30);
        let start = std::time::Instant::now();

        for line in reader.lines() {
            if start.elapsed() > timeout {
                return Err("Timeout waiting for DevTools URL".into());
            }

            let line = line?;
            if line.contains("DevTools listening on") {
                if let Some(url_start) = line.find("ws://") {
                    let url = line[url_start..].trim().to_string();
                    return Ok(url);
                }
            }
        }

        Err("DevTools URL not found in output".into())
    }
}

impl Default for ChromiumLauncher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BrowserLauncher for ChromiumLauncher {
    fn kind(&self) -> BackendKind {
        BackendKind::Chromium
    }

    async fn launch(&mut self) -> BackendResult<()> {
        // Kill any leftover debug processes
        Self::kill_existing_debug_processes();

        let binary = Self::find_binary().ok_or_else(|| {
            BackendError::LaunchFailed("Could not find Chrome/Chromium binary".into())
        })?;

        // Create user data directory
        let user_data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("abrowser")
            .join("chrome-profile");
        std::fs::create_dir_all(&user_data_dir).ok();

        let mut cmd = Command::new(&binary);
        cmd.args([
            "--headless=new",
            "--no-sandbox",
            "--disable-dev-shm-usage",
            // Accessibility
            "--enable-features=Accessibility",
            "--force-renderer-accessibility",
            // Media playback
            "--autoplay-policy=no-user-gesture-required",
            "--enable-features=AudioServiceOutOfProcess",
            "--disable-features=PreloadMediaEngagementData,MediaEngagementBypassAutoplayPolicies",
            // Hide automation/headless detection
            "--disable-blink-features=AutomationControlled",
            "--disable-infobars",
            "--excludeSwitches=enable-automation",
            // Pretend to have GPU
            "--use-gl=swiftshader",
            "--enable-webgl",
            // Window size
            &format!("--window-size={},{}", self.viewport.0, self.viewport.1),
            // User agent
            "--user-agent=Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
            // Persist login sessions
            &format!("--user-data-dir={}", user_data_dir.display()),
            &format!("--remote-debugging-port={}", self.debug_port),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            BackendError::LaunchFailed(format!("Failed to spawn Chromium: {}", e))
        })?;

        let stderr = child.stderr.take().ok_or_else(|| {
            BackendError::LaunchFailed("Failed to capture stderr".into())
        })?;
        let reader = BufReader::new(stderr);

        let ws_url = Self::find_ws_url(reader).map_err(|e| {
            BackendError::LaunchFailed(format!("Failed to get WebSocket URL: {}", e))
        })?;

        // Connect to browser
        let browser_client = CdpClient::connect(&ws_url).await.map_err(|e| {
            BackendError::Connection(format!("Failed to connect to browser: {}", e))
        })?;

        self.browser_ws_url = Some(ws_url);
        self.browser_client = Some(browser_client);
        self.process = Some(child);

        Ok(())
    }

    async fn create_page(&self, url: &str) -> BackendResult<Box<dyn PageSession>> {
        let browser_client = self.browser_client.as_ref().ok_or_else(|| {
            BackendError::NotConnected
        })?;

        // Create a new target with about:blank first (so we can set viewport before loading)
        let result = browser_client
            .call("Target.createTarget", serde_json::json!({ "url": "about:blank" }))
            .await
            .map_err(|e| BackendError::Protocol(format!("Failed to create target: {}", e)))?;

        let target_id = result
            .get("targetId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BackendError::Protocol("No targetId in response".into()))?;

        // Build the page WebSocket URL
        let page_ws_url = format!(
            "ws://127.0.0.1:{}/devtools/page/{}",
            self.debug_port, target_id
        );

        // Connect to page
        let page_client = CdpClient::connect(&page_ws_url).await.map_err(|e| {
            BackendError::Connection(format!("Failed to connect to page: {}", e))
        })?;

        // Set viewport BEFORE navigating
        let (width, height) = self.viewport;
        let is_mobile = width < 800; // Mobile if width is small
        let user_agent = if is_mobile {
            "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1"
        } else {
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
        };

        // Set device metrics
        let _ = page_client
            .call("Emulation.setDeviceMetricsOverride", serde_json::json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": if is_mobile { 2 } else { 1 },
                "mobile": is_mobile
            }))
            .await;

        // Set user agent
        let _ = page_client
            .call("Emulation.setUserAgentOverride", serde_json::json!({
                "userAgent": user_agent
            }))
            .await;

        // Create session (this enables domains)
        let session = ChromiumSession::new(page_client, url.to_string()).await?;

        // NOW navigate to the actual URL
        session.navigate(url).await?;

        Ok(Box::new(session))
    }

    fn set_viewport(&mut self, width: u32, height: u32) {
        self.viewport = (width, height);
    }

    async fn shutdown(&mut self) -> BackendResult<()> {
        // Close browser client
        self.browser_client = None;

        // Kill process
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
        }

        Ok(())
    }

    fn is_running(&self) -> bool {
        self.process
            .as_ref()
            .map(|p| {
                // Check if process is still running by trying to get its status
                std::process::Command::new("kill")
                    .args(["-0", &p.id().to_string()])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }
}

impl Drop for ChromiumLauncher {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
        }
    }
}
