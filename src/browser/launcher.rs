// Headless Chrome launcher

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

pub struct BrowserLauncher {
    process: Option<Child>,
    debug_port: u16,
    browser_ws_url: Option<String>,
    viewport: (u32, u32),
}

impl BrowserLauncher {
    pub fn new() -> Self {
        // Find an available port
        let port = Self::find_available_port().unwrap_or(9222);
        Self {
            process: None,
            debug_port: port,
            browser_ws_url: None,
            viewport: (375, 812), // Default to mobile
        }
    }

    pub fn with_viewport(mut self, width: u32, height: u32) -> Self {
        self.viewport = (width, height);
        self
    }

    /// Find an available port for debugging
    fn find_available_port() -> Option<u16> {
        // Try to bind to port 0 to get an available port
        TcpListener::bind("127.0.0.1:0")
            .ok()
            .and_then(|listener| listener.local_addr().ok())
            .map(|addr| addr.port())
    }

    /// Kill any existing Chromium processes with remote debugging enabled
    fn kill_existing_debug_processes() {
        #[cfg(unix)]
        {
            // Kill any chromium/chrome processes with remote-debugging flag
            let _ = Command::new("pkill")
                .args(["-9", "-f", "remote-debugging-port"])
                .output();
            // Give processes time to die
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Find the headless_shell binary
    fn find_binary() -> Option<PathBuf> {
        // Check common locations
        // Prioritize full chromium over headless-shell on Linux (for audio support)
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

    /// Launch the browser and return the browser-level WebSocket URL
    pub fn launch(&mut self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Kill any leftover debug processes from previous runs
        Self::kill_existing_debug_processes();

        let binary = Self::find_binary().ok_or("Could not find Chrome/Chromium binary")?;

        // Create user data directory to persist sessions/cookies
        let user_data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("abrowser")
            .join("chrome-profile");
        std::fs::create_dir_all(&user_data_dir).ok();

        let mut cmd = Command::new(&binary);
        cmd.args([
            // "--headless=new",  // Disabled to test mouse events
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
            // Pretend to have GPU (helps with WebGL detection)
            "--use-gl=swiftshader",
            "--enable-webgl",
            // Window size
            &format!("--window-size={},{}", self.viewport.0, self.viewport.1),
            // Realistic user agent (don't include HeadlessChrome)
            "--user-agent=Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
            // Persist login sessions
            &format!("--user-data-dir={}", user_data_dir.display()),
            &format!("--remote-debugging-port={}", self.debug_port),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;

        // Read stderr to find the DevTools WebSocket URL
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;
        let reader = BufReader::new(stderr);

        let ws_url = Self::find_ws_url(reader)?;
        self.browser_ws_url = Some(ws_url.clone());
        self.process = Some(child);

        Ok(ws_url)
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
            // Look for "DevTools listening on ws://..."
            if line.contains("DevTools listening on") {
                if let Some(url_start) = line.find("ws://") {
                    let url = line[url_start..].trim().to_string();
                    return Ok(url);
                }
            }
        }

        Err("DevTools URL not found in output".into())
    }

    /// Create a new page target and return its WebSocket URL
    pub async fn create_page(
        &self,
        browser_client: &crate::cdp::CdpClient,
        url: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        use serde_json::json;

        // Create a new target (page)
        let result = browser_client
            .call("Target.createTarget", json!({ "url": url }))
            .await?;

        let target_id = result
            .get("targetId")
            .and_then(|v| v.as_str())
            .ok_or("No targetId in response")?;

        // Build the page WebSocket URL from the target ID
        let page_ws_url = format!(
            "ws://127.0.0.1:{}/devtools/page/{}",
            self.debug_port, target_id
        );

        Ok(page_ws_url)
    }

    /// Wait for page to finish loading
    pub async fn wait_for_load(
        &self,
        page_client: &crate::cdp::CdpClient,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use serde_json::json;

        // Enable Page domain
        page_client.call("Page.enable", json!({})).await?;

        // Wait for load event with timeout
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(30);

        loop {
            if start.elapsed() > timeout {
                // If timeout, just proceed - page may already be loaded
                crate::utils::log::log("Note: Timeout waiting for load event, proceeding anyway");
                break;
            }

            match tokio::time::timeout(Duration::from_millis(500), page_client.recv_event()).await {
                Ok(Some(event)) => {
                    if event.method == "Page.loadEventFired"
                        || event.method == "Page.domContentEventFired"
                    {
                        break;
                    }
                }
                Ok(None) => {
                    // Channel closed, proceed
                    break;
                }
                Err(_) => {
                    // Timeout on recv, continue polling
                    continue;
                }
            }
        }

        Ok(())
    }

    /// Get the browser WebSocket URL if launched
    pub fn browser_ws_url(&self) -> Option<&str> {
        self.browser_ws_url.as_deref()
    }
}

impl Default for BrowserLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for BrowserLauncher {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
        }
    }
}
