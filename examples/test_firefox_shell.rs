//! Simple test of Firefox backend with Shell-like operations
//!
//! Run with: cargo run --example test_firefox_shell

use abrowser::backend::{BackendKind, create_launcher, PageSession};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Testing Firefox backend for shell integration...\n");

    // Create Firefox launcher (same as Shell::new does)
    let mut launcher = create_launcher(BackendKind::Firefox, Some((1024, 768)))?;
    println!("Launcher created");

    // Launch Firefox (same as Shell::run does)
    println!("Launching Firefox...");
    launcher.launch().await?;
    println!("Firefox launched!\n");

    // Create a page session (same as shell's open_url)
    println!("Creating page session for https://example.com...");
    let session: Box<dyn PageSession> = launcher.create_page("https://example.com").await?;
    println!("Page session created!\n");

    // Wait for page to load
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Get URL and title
    let url = session.current_url().await?;
    let title = session.title().await?;
    println!("URL: {}", url);
    println!("Title: {}\n", title);

    // Get accessibility tree (what Shell::force_refresh_tree does)
    println!("Getting accessibility tree...");
    let tree = session.get_accessibility_tree().await?;
    println!("Tree retrieved!\n");

    // Print tree summary
    if let Some(root) = tree.root() {
        println!("Root node: {:?} - {}", root.role, root.name);

        // Count nodes
        let nodes = tree.linearize();
        println!("Total linearized nodes: {}", nodes.len());

        // Print first few
        println!("\nFirst 10 nodes:");
        for (i, node) in nodes.iter().take(10).enumerate() {
            println!("  {}. {:?}: {}", i + 1, node.role,
                if node.name.len() > 50 { format!("{}...", &node.name[..50]) } else { node.name.clone() });
        }
    } else {
        println!("No root node found!");
    }

    // Test event polling (what Shell::process_cdp_events does)
    println!("\nChecking for events...");
    let mut event_count = 0;
    while let Some(event) = session.try_recv_event() {
        println!("  Event: {:?}", event);
        event_count += 1;
        if event_count > 5 { break; }
    }
    if event_count == 0 {
        println!("  No events (this is normal)");
    }

    // Shutdown
    println!("\nShutting down...");
    launcher.shutdown().await?;
    println!("Done!");

    Ok(())
}
