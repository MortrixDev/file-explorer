use std::io;

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::*;

use crate::kitty;

pub type Term = Terminal<CrosstermBackend<io::Stdout>>;

pub fn enter() -> io::Result<Term> {
    enter_screen()?;
    Terminal::new(CrosstermBackend::new(io::stdout()))
}

pub fn enter_screen() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)
}

pub fn leave() -> io::Result<()> {
    let _ = kitty::clear(&mut io::stdout());
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    let main = std::thread::current().id();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() == main {
            let _ = leave();
        }
        previous(info);
    }));
}
