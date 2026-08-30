use std::io;

use crossterm::cursor::SetCursorStyle;
use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
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
    execute!(io::stdout(), EnterAlternateScreen)?;

    let _ = execute!(
        io::stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    Ok(())
}

pub fn cursor_shape(bar: bool) {
    let style = if bar {
        SetCursorStyle::BlinkingBar
    } else {
        SetCursorStyle::DefaultUserShape
    };
    let _ = execute!(io::stdout(), style);
}

pub fn leave() -> io::Result<()> {
    let _ = kitty::clear_all(&mut io::stdout());
    cursor_shape(false);
    let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
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
