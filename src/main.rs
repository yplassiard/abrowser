//! abrowser - Accessible terminal browser
//!
//! Main entry point for the CLI application.

use abrowser::cli::Args;
use abrowser::shell::{Config, Shell};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();

    // Load configuration
    let config = Config::load();

    // Create and run the interactive shell
    let mut shell = Shell::new(config);
    shell.run(args.url.as_deref()).await?;

    Ok(())
}
