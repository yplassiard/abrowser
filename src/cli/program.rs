//! Program entry point handling.

use super::CommandLine;

#[derive(Clone, Debug)]
pub enum CommandLineProgram {
    Main,
    Help,
    Version,
}

impl CommandLineProgram {
    pub fn parse_or_run() -> Option<CommandLine> {
        let cmd = CommandLine::parse();

        match cmd.program {
            CommandLineProgram::Main => return Some(cmd),
            CommandLineProgram::Help => {
                println!("{}", USAGE);
            }
            CommandLineProgram::Version => {
                println!("abrowser {}", env!("CARGO_PKG_VERSION"));
            }
        }

        None
    }
}

const USAGE: &str = r#"abrowser - Accessible terminal browser

USAGE:
    abrowser [OPTIONS] [URL]

OPTIONS:
    -t, --text           Lynx-style text output with numbered links (default)
    -s, --screen-reader  Screen reader optimized output
    -b, --braille[=N]    Braille display output (N cells per line, default 40)
    -d, --debug          Enable debug logging to stderr
    -h, --help           Display this help message
    -v, --version        Show version number

NAVIGATION:
    Tab / Shift+Tab      Move between links
    Enter                Activate link/button
    Arrow keys           Scroll / navigate
    Backspace            Go back
    Ctrl+L               Focus address bar
    Ctrl+C               Exit

EXAMPLES:
    abrowser https://example.com
    abrowser --screen-reader https://news.ycombinator.com
    abrowser --braille=80 https://wikipedia.org
"#;
