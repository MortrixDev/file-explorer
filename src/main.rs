use std::io;
use std::os::unix::ffi::OsStrExt;

use file_explorer::app::{self, Exit};
use file_explorer::cli;
use file_explorer::state::State;
use file_explorer::term;
use file_explorer::theme;

fn main() -> io::Result<()> {
    let args = cli::parse()?;
    let start = match args.dir {
        Some(dir) => std::fs::canonicalize(dir)?,
        None => std::env::current_dir()?,
    };
    let mut state = State::new(start)?;

    term::install_panic_hook();
    let mut terminal = term::enter()?;
    state.stripe = theme::stripe();
    let result = app::run(&mut terminal, &mut state);
    term::leave()?;

    if let Ok(Exit::Cd) = result
        && let Some(path) = &args.cwd_file
    {
        std::fs::write(path, state.cwd.as_os_str().as_bytes())?;
    }
    result.map(|_| ())
}
