//! Firefox RDP protocol debugging tool
//!
//! Run with: cargo run --example firefox_debug

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn main() -> std::io::Result<()> {
    println!("Connecting to Firefox RDP on port 6000...");

    let mut stream = TcpStream::connect("127.0.0.1:6000")?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;  // Increase timeout
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;

    // Read the greeting
    println!("\n=== Reading greeting ===");
    let greeting = read_message(&mut stream)?;
    println!("{}", serde_json::to_string_pretty(&greeting).unwrap_or_default());

    // Try to list processes to find parent accessibility
    println!("\n=== listProcesses ===");
    let processes = send_message(&mut stream, "root", "listProcesses", serde_json::json!({}))?;
    println!("{}", serde_json::to_string_pretty(&processes).unwrap_or_default());

    // Find the parent process and get its target
    if let Some(procs) = processes.get("processes").and_then(|p| p.as_array()) {
        if let Some(parent) = procs.iter().find(|p| p.get("isParent").and_then(|v| v.as_bool()) == Some(true)) {
            if let Some(parent_actor) = parent.get("actor").and_then(|a| a.as_str()) {
                println!("\n=== Parent process: {} ===", parent_actor);

                // Get target from parent process
                println!("\n=== getTarget (parent process) ===");
                let parent_target = send_message(&mut stream, parent_actor, "getTarget", serde_json::json!({}))?;
                println!("{}", serde_json::to_string_pretty(&parent_target).unwrap_or_default());

                // Look for accessibilityActor in the parent process
                if let Some(parent_acc) = parent_target.get("process").and_then(|p| p.get("accessibilityActor")).and_then(|a| a.as_str()) {
                    println!("\n=== Found parent accessibilityActor: {} ===", parent_acc);

                    // Enable accessibility on the parent process's accessibility actor
                    println!("\n=== enable (parent accessibility) ===");
                    let enable_result = send_message(&mut stream, parent_acc, "enable", serde_json::json!({}))?;
                    println!("{}", serde_json::to_string_pretty(&enable_result).unwrap_or_default());

                    // Also try bootstrap on parent
                    println!("\n=== bootstrap (parent accessibility) ===");
                    let bootstrap_result = send_message(&mut stream, parent_acc, "bootstrap", serde_json::json!({}))?;
                    println!("{}", serde_json::to_string_pretty(&bootstrap_result).unwrap_or_default());
                }
            }
        }
    }

    // List tabs
    println!("\n=== Listing tabs ===");
    let tabs_response = send_message(&mut stream, "root", "listTabs", serde_json::json!({}))?;
    println!("{}", serde_json::to_string_pretty(&tabs_response).unwrap_or_default());

    // Get the selected tab (or first non-zombie tab)
    let tabs = tabs_response["tabs"].as_array()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "No tabs array"))?;

    let tab = tabs.iter()
        .find(|t| t["selected"].as_bool() == Some(true))
        .or_else(|| tabs.iter().find(|t| t["isZombieTab"].as_bool() != Some(true)))
        .or_else(|| tabs.first())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "No tabs"))?;

    let tab_actor = tab["actor"].as_str()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "No tab actor"))?;
    println!("\nUsing tab: {} (selected: {:?}, zombie: {:?})",
        tab_actor,
        tab["selected"].as_bool(),
        tab["isZombieTab"].as_bool());

    // Get target from tab
    println!("\n=== Getting target ===");
    let target_response = send_message(&mut stream, tab_actor, "getTarget", serde_json::json!({}))?;
    println!("{}", serde_json::to_string_pretty(&target_response).unwrap_or_default());

    // Try to find the target actor
    let target_actor = if let Some(frame) = target_response.get("frame") {
        frame["actor"].as_str()
    } else if let Some(frames) = target_response.get("frames") {
        frames[0]["actor"].as_str()
    } else {
        None
    };

    if let Some(target) = target_actor {
        println!("\nTarget actor: {}", target);

        // Attach to target
        println!("\n=== Attaching to target ===");
        let attach_response = send_message(&mut stream, target, "attach", serde_json::json!({}))?;
        println!("{}", serde_json::to_string_pretty(&attach_response).unwrap_or_default());

        // Try getFront for accessibility
        println!("\n=== getFront(accessibility) ===");
        let front_response = send_message(&mut stream, target, "getFront",
            serde_json::json!({"typeName": "accessibility"}))?;
        println!("{}", serde_json::to_string_pretty(&front_response).unwrap_or_default());

        // Try listFronts
        println!("\n=== listFronts ===");
        let list_response = send_message(&mut stream, target, "listFronts", serde_json::json!({}))?;
        println!("{}", serde_json::to_string_pretty(&list_response).unwrap_or_default());

        // Look at the target's available methods
        println!("\n=== Target actor info ===");
        // The target might have an accessibility property
        if let Some(acc) = target_response.get("frame").and_then(|f| f.get("traits")) {
            println!("Frame traits: {}", serde_json::to_string_pretty(&acc).unwrap_or_default());
        }

        // Try getAccessibility on target
        println!("\n=== getAccessibility ===");
        let acc_response = send_message(&mut stream, target, "getAccessibility", serde_json::json!({}))?;
        println!("{}", serde_json::to_string_pretty(&acc_response).unwrap_or_default());

        // Check for accessibility in the frame (from getTarget)
        if let Some(acc_actor) = target_response.get("frame").and_then(|f| f.get("accessibilityActor")).and_then(|a| a.as_str()) {
            println!("\n=== Found accessibility actor: {} ===", acc_actor);

            // Use JavaScript to get accessibility info (AOM - Accessibility Object Model)
            if let Some(console) = target_response.get("frame").and_then(|f| f.get("consoleActor")).and_then(|a| a.as_str()) {
                println!("\n=== Getting page content via JS ===");

                // Get all elements with accessibility-relevant info using a simple tree walk
                let script = r#"
                    (function() {
                        function getRole(el) {
                            if (el.role) return el.role;
                            if (el.getAttribute && el.getAttribute('role')) return el.getAttribute('role');
                            const tag = el.tagName?.toLowerCase() || '';
                            const implicitRoles = {
                                'a': el.href ? 'link' : null,
                                'button': 'button',
                                'input': el.type === 'checkbox' ? 'checkbox' :
                                         el.type === 'radio' ? 'radio' :
                                         el.type === 'submit' ? 'button' :
                                         el.type === 'text' ? 'textbox' : 'textbox',
                                'h1': 'heading', 'h2': 'heading', 'h3': 'heading',
                                'h4': 'heading', 'h5': 'heading', 'h6': 'heading',
                                'p': 'paragraph',
                                'ul': 'list', 'ol': 'list',
                                'li': 'listitem',
                                'img': 'img',
                                'nav': 'navigation',
                                'main': 'main',
                                'header': 'banner',
                                'footer': 'contentinfo',
                                'article': 'article',
                                'section': 'region',
                                'form': 'form'
                            };
                            return implicitRoles[tag] || null;
                        }

                        function getName(el) {
                            if (el.ariaLabel) return el.ariaLabel;
                            if (el.getAttribute && el.getAttribute('aria-label')) return el.getAttribute('aria-label');
                            if (el.alt) return el.alt;
                            if (el.title) return el.title;
                            if (el.textContent && el.children.length === 0) return el.textContent.trim().slice(0, 100);
                            return '';
                        }

                        function walk(el, depth = 0, results = []) {
                            if (!el || depth > 10) return results;
                            const role = getRole(el);
                            const name = getName(el);
                            if (role || name) {
                                results.push({
                                    tag: el.tagName || '#text',
                                    role: role,
                                    name: name,
                                    depth: depth
                                });
                            }
                            for (const child of (el.children || [])) {
                                walk(child, depth + 1, results);
                            }
                            return results;
                        }

                        return walk(document.body || document.documentElement);
                    })()
                "#;

                let js_result = send_message(&mut stream, console, "evaluateJSAsync", serde_json::json!({
                    "text": script,
                    "eager": false
                }))?;
                println!("AOM result: {}", serde_json::to_string_pretty(&js_result).unwrap_or_default());
            }

            // Bootstrap the accessibility service
            println!("\n=== bootstrap ===");
            let bootstrap_response = send_message(&mut stream, acc_actor, "bootstrap", serde_json::json!({}))?;
            println!("{}", serde_json::to_string_pretty(&bootstrap_response).unwrap_or_default());

            // Try using inspector to get DOM and accessibility
            if let Some(inspector) = target_response.get("frame").and_then(|f| f.get("inspectorActor")).and_then(|a| a.as_str()) {
                println!("\n=== Using inspector: {} ===", inspector);

                // Get DOM walker from inspector
                println!("\n=== getWalker (DOM) ===");
                let dom_walker = send_message(&mut stream, inspector, "getWalker", serde_json::json!({}))?;
                println!("{}", serde_json::to_string_pretty(&dom_walker).unwrap_or_default());

                // Get document from DOM walker
                if let Some(walker_actor) = dom_walker.get("walker").and_then(|w| w.get("actor")).and_then(|a| a.as_str()) {
                    println!("\n=== document (DOM) ===");
                    let doc = send_message(&mut stream, walker_actor, "document", serde_json::json!({}))?;
                    println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());

                    // Get children of document
                    if let Some(doc_node) = doc.get("node").and_then(|n| n.get("actor")).and_then(|a| a.as_str()) {
                        println!("\n=== children (DOM root) ===");
                        let children = send_message(&mut stream, walker_actor, "children",
                            serde_json::json!({"node": doc_node}))?;
                        println!("{}", serde_json::to_string_pretty(&children).unwrap_or_default());

                        // Now try to get accessibility for a DOM node
                        if let Some(nodes) = children.get("nodes").and_then(|n| n.as_array()) {
                            if let Some(first) = nodes.first() {
                                let node_actor = first.get("actor").and_then(|a| a.as_str());
                                if let Some(na) = node_actor {
                                    println!("\n=== First DOM child: {} ===", na);
                                    println!("NodeType: {:?}", first.get("nodeType"));
                                    println!("NodeName: {:?}", first.get("nodeName"));

                                    // Try to get accessibility properties via the walker
                                    println!("\n=== getAccessibleFor (DOM node) ===");
                                    // The getAccessibleFor is on the accessibility walker
                                    let walker_response = send_message(&mut stream, acc_actor, "getWalker", serde_json::json!({}))?;
                                    if let Some(acc_walker) = walker_response.get("walker").and_then(|w| w.get("actor")).and_then(|a| a.as_str()) {
                                        let acc_for_node = send_message(&mut stream, acc_walker, "getAccessibleFor",
                                            serde_json::json!({"node": na}))?;
                                        println!("{}", serde_json::to_string_pretty(&acc_for_node).unwrap_or_default());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            println!("\n=== No accessibility actor found in frame ===");
        }
    }

    Ok(())
}

fn send_message(stream: &mut TcpStream, to: &str, method: &str, params: serde_json::Value) -> std::io::Result<serde_json::Value> {
    let mut msg = params;
    if let Some(obj) = msg.as_object_mut() {
        obj.insert("to".to_string(), serde_json::Value::String(to.to_string()));
        obj.insert("type".to_string(), serde_json::Value::String(method.to_string()));
    }

    let json = serde_json::to_string(&msg)?;
    let packet = format!("{}:{}", json.len(), json);

    println!(">> {}", json);
    stream.write_all(packet.as_bytes())?;
    stream.flush()?;

    // Read until we get a response from the expected actor
    loop {
        let response = read_message(stream)?;
        let from = response.get("from").and_then(|f| f.as_str()).unwrap_or("");

        if from == to {
            return Ok(response);
        }

        // Print any events/responses from other actors for debugging
        println!("  [EVENT from {}]: {}", from,
            response.get("type").and_then(|t| t.as_str()).unwrap_or("unknown"));
    }
}

fn read_message(stream: &mut TcpStream) -> std::io::Result<serde_json::Value> {
    // Read length prefix
    let mut length_buf = Vec::new();
    let mut byte = [0u8; 1];

    loop {
        stream.read_exact(&mut byte)?;
        if byte[0] == b':' {
            break;
        }
        length_buf.push(byte[0]);
    }

    let length: usize = String::from_utf8_lossy(&length_buf)
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Invalid length: {}", e)))?;

    // Read JSON body
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body)?;

    let json: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Invalid JSON: {}", e)))?;

    Ok(json)
}
