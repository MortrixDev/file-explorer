use std::io;
use std::path::PathBuf;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::theme;

const USAGE: &str = "\
usage: file-explorer [DIR] [--cwd-file PATH]
       file-explorer init <bash|zsh|fish|nu>
       file-explorer probe

  DIR              directory to open (default: current)
  --cwd-file PATH  on exit, write the final directory here (q; Q to skip)
  init SHELL       print a shell function that cds where you left off
  probe            report the detected background and stripe colour
";

pub struct Args {
    pub dir: Option<PathBuf>,
    pub cwd_file: Option<PathBuf>,
}

pub fn parse() -> io::Result<Args> {
    let mut args = std::env::args_os().skip(1);
    let mut parsed = Args {
        dir: None,
        cwd_file: None,
    };

    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-h" | "--help") => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            Some("probe") => {
                enable_raw_mode()?;
                let report = theme::probe();
                disable_raw_mode()?;
                println!("{report}");
                std::process::exit(0);
            }
            Some("init") => {
                let shell = args.next();
                print_init(shell.as_ref().and_then(|s| s.to_str()))?;
                std::process::exit(0);
            }
            Some("--cwd-file") => {
                let path = args.next().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--cwd-file needs a path")
                })?;
                parsed.cwd_file = Some(PathBuf::from(path));
            }
            Some(flag) if flag.starts_with('-') => usage_error(&format!("unknown option {flag}")),
            _ => {
                if parsed.dir.is_some() {
                    usage_error("only one directory may be given");
                }
                parsed.dir = Some(PathBuf::from(arg));
            }
        }
    }
    Ok(parsed)
}

fn print_init(shell: Option<&str>) -> io::Result<()> {
    let template = match shell {
        Some("nu" | "nushell") => include_str!("../shells/init.nu"),
        Some("bash" | "zsh") => include_str!("../shells/init.bash"),
        Some("fish") => include_str!("../shells/init.fish"),
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "init: unknown shell {:?}, expected bash, zsh, fish or nu",
                    other.unwrap_or("")
                ),
            ));
        }
    };

    let bin = std::env::current_exe()
        .ok()
        .and_then(|path| path.into_os_string().into_string().ok())
        .unwrap_or_else(|| "file-explorer".to_string());

    print!("{}", template.replace("@BIN@", &bin));
    Ok(())
}

fn usage_error(message: &str) -> ! {
    eprintln!("file-explorer: {message}\n");
    eprint!("{USAGE}");
    std::process::exit(2);
}
