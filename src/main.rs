//! abrowser - Accessible terminal browser
//!
//! Main entry point for the CLI application.

use abrowser::cli::Args;
use abrowser::shell::{Config, Shell};
use crossterm::{cursor, execute, terminal};
use std::io::Write;
use std::panic;

/// Cleanup terminal state - called on panic
fn cleanup_terminal() {
    let _ = execute!(std::io::stdout(), terminal::LeaveAlternateScreen, cursor::Show);
    let _ = terminal::disable_raw_mode();
    let _ = std::io::stdout().flush();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Set up panic hook to clean up terminal
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        cleanup_terminal();
        default_hook(info);
    }));

    let args = Args::parse();

    // Load configuration and apply CLI overrides
    let mut config = Config::load();
    config.backend = args.backend;

    // Create and run the interactive shell
    let mut shell = Shell::new(config)?;
    shell.run(args.url.as_deref()).await?;

    Ok(())
}
