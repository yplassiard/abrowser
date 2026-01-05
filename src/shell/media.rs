//! Media playback control via CDP
//! Controls video/audio elements on any webpage using JavaScript injection

use crate::cdp::CdpClient;
use serde_json::json;

/// Media controller for video/audio elements
pub struct MediaController;

impl MediaController {
    /// Toggle play/pause on the current video
    pub async fn toggle_play(client: &CdpClient) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let result = client.call("Runtime.evaluate", json!({
            "expression": r#"
                (function() {
                    const video = document.querySelector('video');
                    if (!video) return { success: false, error: 'No video found' };
                    if (video.paused) {
                        video.play();
                        return { success: true, playing: true };
                    } else {
                        video.pause();
                        return { success: true, playing: false };
                    }
                })()
            "#,
            "returnByValue": true
        })).await?;

        Ok(result.get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.get("playing"))
            .and_then(|p| p.as_bool())
            .unwrap_or(false))
    }

    /// Play video
    pub async fn play(client: &CdpClient) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        client.call("Runtime.evaluate", json!({
            "expression": "document.querySelector('video')?.play()"
        })).await?;
        Ok(())
    }

    /// Pause video
    pub async fn pause(client: &CdpClient) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        client.call("Runtime.evaluate", json!({
            "expression": "document.querySelector('video')?.pause()"
        })).await?;
        Ok(())
    }

    /// Seek relative (positive = forward, negative = backward)
    pub async fn seek_relative(client: &CdpClient, seconds: f64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        client.call("Runtime.evaluate", json!({
            "expression": format!(
                "(() => {{ const v = document.querySelector('video'); if (v) v.currentTime += {}; }})()",
                seconds
            )
        })).await?;
        Ok(())
    }

    /// Seek to percentage (0-100)
    pub async fn seek_percent(client: &CdpClient, percent: u8) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        client.call("Runtime.evaluate", json!({
            "expression": format!(
                "(() => {{ const v = document.querySelector('video'); if (v) v.currentTime = v.duration * {} / 100; }})()",
                percent
            )
        })).await?;
        Ok(())
    }

    /// Toggle mute
    pub async fn toggle_mute(client: &CdpClient) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let result = client.call("Runtime.evaluate", json!({
            "expression": r#"
                (function() {
                    const video = document.querySelector('video');
                    if (!video) return { muted: false };
                    video.muted = !video.muted;
                    return { muted: video.muted };
                })()
            "#,
            "returnByValue": true
        })).await?;

        Ok(result.get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.get("muted"))
            .and_then(|m| m.as_bool())
            .unwrap_or(false))
    }

    /// Adjust volume (delta: -0.1 to +0.1 typically)
    pub async fn adjust_volume(client: &CdpClient, delta: f64) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        let result = client.call("Runtime.evaluate", json!({
            "expression": format!(
                r#"
                (function() {{
                    const video = document.querySelector('video');
                    if (!video) return {{ volume: 0 }};
                    video.volume = Math.max(0, Math.min(1, video.volume + {}));
                    return {{ volume: video.volume }};
                }})()
                "#,
                delta
            ),
            "returnByValue": true
        })).await?;

        Ok(result.get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.get("volume"))
            .and_then(|vol| vol.as_f64())
            .unwrap_or(0.0))
    }

    /// Get current playback status
    pub async fn get_status(client: &CdpClient) -> Result<MediaStatus, Box<dyn std::error::Error + Send + Sync>> {
        let result = client.call("Runtime.evaluate", json!({
            "expression": r#"
                (function() {
                    const video = document.querySelector('video');
                    if (!video) return null;
                    return {
                        playing: !video.paused,
                        currentTime: video.currentTime,
                        duration: video.duration,
                        muted: video.muted,
                        volume: video.volume,
                        title: document.title
                    };
                })()
            "#,
            "returnByValue": true
        })).await?;

        if let Some(value) = result.get("result").and_then(|r| r.get("value")) {
            if value.is_null() {
                return Ok(MediaStatus::default());
            }
            Ok(MediaStatus {
                has_video: true,
                playing: value.get("playing").and_then(|v| v.as_bool()).unwrap_or(false),
                current_time: value.get("currentTime").and_then(|v| v.as_f64()).unwrap_or(0.0),
                duration: value.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0),
                muted: value.get("muted").and_then(|v| v.as_bool()).unwrap_or(false),
                volume: value.get("volume").and_then(|v| v.as_f64()).unwrap_or(1.0),
                title: value.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
        } else {
            Ok(MediaStatus::default())
        }
    }

    /// Set playback speed
    pub async fn set_speed(client: &CdpClient, speed: f64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        client.call("Runtime.evaluate", json!({
            "expression": format!(
                "(() => {{ const v = document.querySelector('video'); if (v) v.playbackRate = {}; }})()",
                speed
            )
        })).await?;
        Ok(())
    }

    /// Toggle fullscreen (for accessibility, announce it)
    pub async fn request_fullscreen(client: &CdpClient) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        client.call("Runtime.evaluate", json!({
            "expression": "document.querySelector('video')?.requestFullscreen()"
        })).await?;
        Ok(())
    }

    /// Toggle captions/subtitles
    pub async fn toggle_captions(client: &CdpClient) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let result = client.call("Runtime.evaluate", json!({
            "expression": r#"
                (function() {
                    const video = document.querySelector('video');
                    if (!video) return { enabled: false };
                    const tracks = video.textTracks;
                    if (tracks.length === 0) return { enabled: false };
                    const track = tracks[0];
                    track.mode = track.mode === 'showing' ? 'hidden' : 'showing';
                    return { enabled: track.mode === 'showing' };
                })()
            "#,
            "returnByValue": true
        })).await?;

        Ok(result.get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.get("enabled"))
            .and_then(|e| e.as_bool())
            .unwrap_or(false))
    }
}

/// Current media playback status
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaStatus {
    pub has_video: bool,
    pub playing: bool,
    pub current_time: f64,
    pub duration: f64,
    pub muted: bool,
    pub volume: f64,
    pub title: String,
}

impl MediaStatus {
    /// Create from backend MediaStatus
    pub fn from_backend(status: crate::backend::MediaStatus) -> Self {
        Self {
            has_video: status.has_video,
            playing: status.playing,
            current_time: status.current_time,
            duration: status.duration,
            muted: status.muted,
            volume: status.volume,
            title: String::new(), // Backend doesn't provide title
        }
    }

    /// Format current time as MM:SS
    pub fn format_time(seconds: f64) -> String {
        if seconds.is_nan() || seconds.is_infinite() {
            return "--:--".to_string();
        }
        let mins = (seconds / 60.0) as u32;
        let secs = (seconds % 60.0) as u32;
        format!("{:02}:{:02}", mins, secs)
    }

    /// Get progress as percentage
    pub fn progress_percent(&self) -> u8 {
        if self.duration > 0.0 {
            ((self.current_time / self.duration) * 100.0) as u8
        } else {
            0
        }
    }

    /// Format status for display
    pub fn format_status(&self) -> String {
        if !self.has_video {
            return String::new();
        }
        let state = if self.playing { "▶" } else { "⏸" };
        let mute = if self.muted { "🔇" } else { "" };
        let time = format!(
            "{} / {}",
            Self::format_time(self.current_time),
            Self::format_time(self.duration)
        );
        format!("{} {} {} [{}%]", state, time, mute, (self.volume * 100.0) as u8)
    }
}
