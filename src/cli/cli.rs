//! Command-line argument parsing for abrowser.

use std::{env, ffi::OsStr};

use crate::backend::BackendKind;
use super::CommandLineProgram;

/// Output mode for accessibility rendering
#[derive(Clone, Debug, Default, PartialEq)]
pub enum OutputMode {
    /// Lynx-style text output with numbered links
    #[default]
    Text,
    /// Screen reader optimized output (semantic announcements)
    ScreenReader,
    /// Braille display output (fixed-width lines)
    Braille,
}

#[derive(Clone, Debug)]
pub struct CommandLine {
    pub args: Vec<String>,
    pub debug: bool,
    pub output_mode: OutputMode,
    pub braille_cells: usize,
    pub program: CommandLineProgram,
    pub shell_mode: bool,
    pub url: Option<String>,
    pub backend: BackendKind,
}

/// Simplified args structure for the main binary
#[derive(Clone, Debug)]
pub struct Args {
    pub mode: OutputMode,
    pub braille_cells: u8,
    pub url: Option<String>,
    pub backend: BackendKind,
}

impl Args {
    pub fn parse() -> Self {
        let cmd = CommandLine::parse();
        Args {
            mode: cmd.output_mode,
            braille_cells: cmd.braille_cells as u8,
            url: cmd.url,
            backend: cmd.backend,
        }
    }
}

pub enum EnvVar {
    Debug,
    ShellMode,
    OutputMode,
    Backend,
}

impl EnvVar {
    pub fn as_str(&self) -> &'static str {
        match self {
            EnvVar::Debug => "ABROWSER_DEBUG",
            EnvVar::ShellMode => "ABROWSER_SHELL_MODE",
            EnvVar::OutputMode => "ABROWSER_OUTPUT_MODE",
            EnvVar::Backend => "ABROWSER_BACKEND",
        }
    }
}

impl AsRef<OsStr> for EnvVar {
    fn as_ref(&self) -> &OsStr {
        self.as_str().as_ref()
    }
}

impl CommandLine {
    pub fn parse() -> CommandLine {
        let mut debug = false;
        let mut shell_mode = false;
        let mut output_mode = OutputMode::Text;
        let mut braille_cells = 40; // Standard braille display width
        let mut program = CommandLineProgram::Main;
        let mut url = None;
        let mut backend = BackendKind::Chromium; // Default to Chromium
        let args: Vec<String> = env::args().skip(1).collect();

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            let split: Vec<&str> = arg.split('=').collect();
            let default = arg.as_str();
            let (key, value) = (split.first().unwrap_or(&default), split.get(1));

            match *key {
                "-d" | "--debug" => {
                    debug = true;
                    env::set_var(EnvVar::Debug, "1");
                }
                "-t" | "--text" => output_mode = OutputMode::Text,
                "-s" | "--screen-reader" => output_mode = OutputMode::ScreenReader,
                "-b" | "--braille" => {
                    output_mode = OutputMode::Braille;
                    // Check for value after = or as next arg
                    let cells_str = value.copied().or_else(|| {
                        if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                            i += 1;
                            Some(args[i].as_str())
                        } else {
                            None
                        }
                    });
                    if let Some(cells) = cells_str {
                        if let Ok(n) = cells.parse::<usize>() {
                            braille_cells = n;
                        }
                    }
                }
                "--braille-cells" => {
                    let cells_str = value.copied().or_else(|| {
                        if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                            i += 1;
                            Some(args[i].as_str())
                        } else {
                            None
                        }
                    });
                    if let Some(cells) = cells_str {
                        if let Ok(n) = cells.parse::<usize>() {
                            braille_cells = n;
                        }
                    }
                }
                "--backend" => {
                    // Support both --backend=firefox and --backend firefox
                    let backend_str = value.copied().or_else(|| {
                        if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                            i += 1;
                            Some(args[i].as_str())
                        } else {
                            None
                        }
                    });
                    if let Some(b) = backend_str {
                        if let Ok(k) = b.parse::<BackendKind>() {
                            backend = k;
                        }
                    }
                }
                "-h" | "--help" => program = CommandLineProgram::Help,
                "-v" | "--version" => program = CommandLineProgram::Version,
                _ => {
                    // Non-flag argument is treated as URL
                    if !arg.starts_with('-') && url.is_none() {
                        url = Some(arg.clone());
                    }
                }
            }
            i += 1;
        }

        if env::var(EnvVar::Debug).is_ok() {
            debug = true;
        }

        if env::var(EnvVar::ShellMode).is_ok() {
            shell_mode = true;
        }

        if let Ok(mode) = env::var(EnvVar::OutputMode) {
            output_mode = match mode.as_str() {
                "text" => OutputMode::Text,
                "screen-reader" => OutputMode::ScreenReader,
                "braille" => OutputMode::Braille,
                _ => output_mode,
            };
        }

        // Environment variable overrides CLI for backend
        if let Ok(b) = env::var(EnvVar::Backend) {
            if let Ok(k) = b.parse::<BackendKind>() {
                backend = k;
            }
        }

        CommandLine {
            args,
            debug,
            output_mode,
            braille_cells,
            program,
            shell_mode,
            url,
            backend,
        }
    }
}
