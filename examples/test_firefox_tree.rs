//! Simple test of Firefox accessibility tree
//!
//! Run with: cargo run --example test_firefox_tree

use abrowser::backend::{BackendKind, create_launcher, PageSession};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing Firefox backend accessibility tree...\n");

    // Create Firefox launcher
    let mut launcher = create_launcher(BackendKind::Firefox, Some((1024, 768)))?;

    // Launch Firefox
    println!("Launching Firefox...");
    launcher.launch().await?;
    println!("Firefox launched!\n");

    // Create a page session for example.com
    println!("Creating page session for example.com...");
    let session: Box<dyn PageSession> = launcher.create_page("https://example.com").await?;
    println!("Page session created!\n");

    // Wait for page to load
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Get URL and title
    let url = session.current_url().await?;
    let title = session.title().await?;
    println!("URL: {}", url);
    println!("Title: {}\n", title);

    // Get accessibility tree
    println!("Getting accessibility tree...");
    let tree: abrowser::accessibility::AXTree = session.get_accessibility_tree().await?;
    println!("Tree retrieved!\n");

    // Print tree summary
    if let Some(root) = tree.root() {
        println!("Root node: {:?} - {}", root.role, root.name);
        println!("Total nodes: {}\n", count_nodes(&tree, &root.id));

        // Print first few interesting nodes
        println!("Interesting nodes:");
        let mut count = 0;
        print_interesting(&tree, &root.id, 0, &mut count, 20);
    } else {
        println!("No root node found!");
    }

    // Shutdown
    println!("\nShutting down...");
    launcher.shutdown().await?;
    println!("Done!");

    Ok(())
}

fn count_nodes(tree: &abrowser::accessibility::AXTree, id: &str) -> usize {
    let mut count = 1;
    if let Some(node) = tree.get(id) {
        for child_id in &node.child_ids {
            count += count_nodes(tree, child_id);
        }
    }
    count
}

fn print_interesting(tree: &abrowser::accessibility::AXTree, id: &str, depth: usize, count: &mut usize, max: usize) {
    if *count >= max {
        return;
    }

    if let Some(node) = tree.get(id) {
        use abrowser::accessibility::Role;
        let interesting = !matches!(node.role, Role::Generic) || !node.name.is_empty();

        if interesting {
            let indent = "  ".repeat(depth);
            let name_preview = if node.name.len() > 50 {
                format!("{}...", &node.name[..50])
            } else {
                node.name.clone()
            };
            println!("{}{:?}: {}", indent, node.role, name_preview);
            *count += 1;
        }

        for child_id in &node.child_ids {
            print_interesting(tree, child_id, depth + 1, count, max);
        }
    }
}
