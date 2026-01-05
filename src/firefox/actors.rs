//! Firefox RDP actor management
//!
//! The Firefox RDP uses an actor model where each entity has an actor ID.
//! This module manages the actor chain for accessibility:
//! root → listTabs → tabDescriptor → getTarget → frame → accessibilityActor

use super::client::RdpClient;
use serde_json::{json, Value};
use std::io;

/// Manages Firefox RDP actors for a page
/// Result of getTarget containing all relevant actors from the frame
#[derive(Debug, Clone)]
pub struct TargetActors {
    pub target: String,
    pub console: Option<String>,
    pub accessibility: Option<String>,
}

/// Manages Firefox RDP actors for a page
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ActorInfo {
    pub root: String,
    pub tab_descriptor: String,
    pub target: String,
    pub console: Option<String>,
    pub accessibility: Option<String>,
    pub accessibility_walker: Option<String>,
    pub parent_accessibility: Option<String>,
}

impl ActorInfo {
    /// Get the tab list and best tab descriptor (selected or first non-zombie)
    pub fn get_tab_descriptor(client: &RdpClient) -> io::Result<String> {
        // Request tab list from root actor
        let response = client.send("root", "listTabs", json!({}))?;

        // Get the tabs array
        let tabs = response
            .get("tabs")
            .and_then(|t| t.as_array())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "No tabs in response"))?;

        if tabs.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "No tabs available"));
        }

        // Prefer selected tab, then non-zombie tab, then first tab
        let tab = tabs.iter()
            .find(|t| t.get("selected").and_then(|v| v.as_bool()) == Some(true))
            .or_else(|| tabs.iter().find(|t| t.get("isZombieTab").and_then(|v| v.as_bool()) != Some(true)))
            .or_else(|| tabs.first())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No usable tab found"))?;

        let tab_actor = tab
            .get("actor")
            .and_then(|a| a.as_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "No actor in tab"))?;

        Ok(tab_actor.to_string())
    }

    /// Get the target actor from a tab descriptor
    pub fn get_target(client: &RdpClient, tab_descriptor: &str) -> io::Result<TargetActors> {
        // Get the target (page) from the tab descriptor
        let response = client.send(tab_descriptor, "getTarget", json!({}))?;

        let frame = response
            .get("frame")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "No frame in target response"))?;

        let target_actor = frame
            .get("actor")
            .and_then(|a| a.as_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "No actor in frame"))?;

        // Console actor is in the frame
        let console_actor = frame
            .get("consoleActor")
            .and_then(|a| a.as_str())
            .map(|s| s.to_string());

        // Accessibility actor is in the frame
        let accessibility_actor = frame
            .get("accessibilityActor")
            .and_then(|a| a.as_str())
            .map(|s| s.to_string());

        Ok(TargetActors {
            target: target_actor.to_string(),
            console: console_actor,
            accessibility: accessibility_actor,
        })
    }

    /// Attach to a target (required before accessing actors)
    pub fn attach_target(client: &RdpClient, target: &str) -> io::Result<Value> {
        client.send(target, "attach", json!({}))
    }

    /// Get the accessibility actor from a target
    pub fn get_accessibility_actor(client: &RdpClient, target: &str) -> io::Result<String> {
        // In Firefox RDP, we need to get the accessibility front using getFront
        // Try getFront method (newer Firefox)
        let response = client.send(target, "getFront", json!({"typeName": "accessibility"}))?;

        if let Some(actor) = response.get("actor").and_then(|a| a.as_str()) {
            return Ok(actor.to_string());
        }

        // Try getActor method (alternative)
        let response = client.send(target, "getActor", json!({"name": "accessibility"}))?;

        if let Some(actor) = response.get("actor").and_then(|a| a.as_str()) {
            return Ok(actor.to_string());
        }

        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Accessibility actor not found",
        ))
    }

    /// Get the parent accessibility actor (browser-level accessibility)
    pub fn get_parent_accessibility(_client: &RdpClient, _target: &str) -> io::Result<String> {
        // Skip parent accessibility - focus on content accessibility
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Parent accessibility not implemented",
        ))
    }

    /// Enable the accessibility actor
    pub fn enable_accessibility(client: &RdpClient, parent_accessibility: &str) -> io::Result<Value> {
        client.send(parent_accessibility, "enable", json!({}))
    }

    /// Get the accessibility walker
    pub fn get_walker(client: &RdpClient, accessibility: &str) -> io::Result<String> {
        let response = client.send(accessibility, "getWalker", json!({}))?;

        let walker = response
            .get("walker")
            .and_then(|w| w.get("actor"))
            .and_then(|a| a.as_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "No walker in response"))?;

        Ok(walker.to_string())
    }

    /// Get the document node from the walker
    pub fn get_document(client: &RdpClient, walker: &str) -> io::Result<Value> {
        client.send(walker, "getDocument", json!({}))
    }

    /// Get children of an accessible node
    pub fn get_children(client: &RdpClient, walker: &str, node: &str) -> io::Result<Vec<Value>> {
        let response = client.send(walker, "children", json!({
            "node": node
        }))?;

        let children = response
            .get("children")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(children)
    }

    /// Get full properties of a node (hydrate)
    #[allow(dead_code)]
    pub fn hydrate_node(client: &RdpClient, node_actor: &str) -> io::Result<Value> {
        client.send(node_actor, "hydrate", json!({}))
    }

    /// Create a new ActorInfo by discovering all actors
    pub fn discover(client: &RdpClient) -> io::Result<Self> {
        // Get tab descriptor with retries (tab may not be available immediately)
        let tab_descriptor = {
            let mut last_error = None;
            let mut tab = None;
            for _attempt in 0..10 {
                match Self::get_tab_descriptor(client) {
                    Ok(t) => {
                        tab = Some(t);
                        break;
                    }
                    Err(e) => {
                        last_error = Some(e);
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                }
            }
            tab.ok_or_else(|| last_error.unwrap_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "No tabs found after retries")
            }))?
        };

        // Get target - this gives us target, console, and accessibility actors
        let target_actors = Self::get_target(client, &tab_descriptor)?;

        // Attach to target
        let _ = Self::attach_target(client, &target_actors.target);

        // Get walker if we have the accessibility actor
        let accessibility_walker = target_actors.accessibility.as_ref().and_then(|a| {
            Self::get_walker(client, a).ok()
        });

        Ok(Self {
            root: "root".to_string(),
            tab_descriptor,
            target: target_actors.target,
            console: target_actors.console,
            accessibility: target_actors.accessibility,
            accessibility_walker,
            parent_accessibility: None,
        })
    }
}

/// Navigate to a URL using the target actor
pub fn navigate(client: &RdpClient, target: &str, url: &str) -> io::Result<Value> {
    client.send(target, "navigateTo", json!({
        "url": url
    }))
}

/// Reload the page
pub fn reload(client: &RdpClient, target: &str) -> io::Result<Value> {
    client.send(target, "reload", json!({}))
}

/// Get the current URL (using tab descriptor, not target)
pub fn get_url(client: &RdpClient, tab_descriptor: &str) -> io::Result<String> {
    let response = client.send(tab_descriptor, "getTarget", json!({}))?;

    let url = response
        .get("frame")
        .and_then(|f| f.get("url"))
        .and_then(|u| u.as_str())
        .unwrap_or("about:blank");

    Ok(url.to_string())
}

/// Get the page title (using tab descriptor, not target)
pub fn get_title(client: &RdpClient, tab_descriptor: &str) -> io::Result<String> {
    let response = client.send(tab_descriptor, "getTarget", json!({}))?;

    let title = response
        .get("frame")
        .and_then(|f| f.get("title"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    Ok(title.to_string())
}

/// Evaluate JavaScript in the page
#[allow(dead_code)]
pub fn evaluate_js(client: &RdpClient, console: &str, script: &str) -> io::Result<Value> {
    let response = client.send(console, "evaluateJSAsync", json!({
        "text": script,
        "eager": false
    }))?;

    // The result might be in different places depending on the response
    if let Some(result) = response.get("result") {
        return Ok(result.clone());
    }

    // Wait for async result
    if let Some(_result_id) = response.get("resultID") {
        // In a real implementation, we'd wait for the result event
        // For now, return the response
        return Ok(response);
    }

    Ok(response)
}

/// Simulate input events
#[allow(dead_code)]
pub fn dispatch_key_event(_client: &RdpClient, _target: &str, _key: &str, _key_code: u32) -> io::Result<()> {
    // Firefox doesn't have a direct input dispatch like CDP
    // We need to use the console to simulate events
    // This is a simplified implementation
    Ok(())
}

/// Click on an element (using JavaScript)
#[allow(dead_code)]
pub fn click_element(client: &RdpClient, console: &str, selector_or_script: &str) -> io::Result<()> {
    let script = if selector_or_script.starts_with("document.") {
        format!("{}?.click()", selector_or_script)
    } else {
        format!("document.querySelector({:?})?.click()", selector_or_script)
    };

    evaluate_js(client, console, &script)?;
    Ok(())
}

/// Focus an element (using JavaScript)
#[allow(dead_code)]
pub fn focus_element(client: &RdpClient, console: &str, selector_or_script: &str) -> io::Result<()> {
    let script = if selector_or_script.starts_with("document.") {
        format!("{}?.focus()", selector_or_script)
    } else {
        format!("document.querySelector({:?})?.focus()", selector_or_script)
    };

    evaluate_js(client, console, &script)?;
    Ok(())
}
