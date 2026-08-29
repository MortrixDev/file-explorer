use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::prelude::*;

use crate::kitty;
use crate::preview::Preview;
use crate::state::{Content, State};
use crate::term::{Term, enter_screen, leave};
use crate::theme;
use crate::tui;

const THEME_INTERVAL: Duration = Duration::from_secs(2);

pub enum Exit {
    Cd,
    Stay,
}

pub fn run(terminal: &mut Term, state: &mut State) -> io::Result<Exit> {
    let mut loader = kitty::Loader::new();
    let mut shown: Option<(PathBuf, Rect)> = None;
    let mut watch_theme = true;
    let mut checked = Instant::now();

    loop {
        let size = terminal.size()?;
        let area = Rect::new(0, 0, size.width, size.height);
        state.left.scroll_into_view(tui::list_height(area));
        state.sync_right();
        state.poll_previews();

        terminal.draw(|frame| tui::draw(frame, state))?;

        loader.poll();
        sync_image(state, &mut loader, &mut shown, tui::preview_area(area));

        let timeout = if loader.busy() || state.busy() {
            20
        } else {
            250
        };
        if !event::poll(Duration::from_millis(timeout))? {
            if watch_theme && checked.elapsed() >= THEME_INTERVAL {
                checked = Instant::now();
                match theme::recheck() {
                    Some(stripe) => state.stripe = stripe,
                    None => watch_theme = false,
                }
            }
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        let page = tui::list_height(area).saturating_sub(1) as isize;
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(Exit::Cd),
            KeyCode::Char('Q') => return Ok(Exit::Stay),
            KeyCode::Char('j') | KeyCode::Down => state.left.move_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => state.left.move_cursor(-1),
            KeyCode::Char('d') => state.left.move_cursor(page),
            KeyCode::Char('u') => state.left.move_cursor(-page),
            KeyCode::Char('g') => state.left.cursor_first(),
            KeyCode::Char('G') => state.left.cursor_last(),
            KeyCode::Char('.') => state.toggle_hidden(),
            KeyCode::Char('e') => {
                if let Some(path) = state.selected_file() {
                    state.error = match open_in_editor(terminal, &path) {
                        Ok(()) => None,
                        Err(e) => Some(e.to_string()),
                    };
                    state.right_of = None;
                    shown = None;
                }
            }
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => state.enter(),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => state.leave(),
            _ => {}
        }
    }
}

fn sync_image(
    state: &mut State,
    loader: &mut kitty::Loader,
    shown: &mut Option<(PathBuf, Rect)>,
    area: Rect,
) {
    let want = match (&state.right, &state.right_of) {
        (Content::File(Preview::Image { .. }), Some(path)) if state.graphics && !state.busy() => {
            Some((path.clone(), area))
        }
        _ => None,
    };

    state.loading = false;

    let mut out = io::stdout();
    if shown.is_some() && *shown != want {
        let _ = kitty::clear(&mut out);
        *shown = None;
    }

    let Some((path, rect)) = want else { return };
    if shown.is_some() {
        return;
    }
    let Some(cell) = kitty::cell_size() else {
        return;
    };

    let key = (
        path.clone(),
        rect.width as u32 * cell.w,
        rect.height as u32 * cell.h,
    );
    if key.1 == 0 || key.2 == 0 {
        return;
    }

    let entry = loader.get(&key);
    match entry.as_deref() {
        Some(Ok(image)) => {
            if let Err(e) = kitty::place(&mut out, image, rect.x, rect.y) {
                state.error = Some(format!("{}: {e}", path.display()));
            }
            *shown = Some((path, rect));
        }
        Some(Err(e)) => {
            state.error = Some(format!("{}: {e}", path.display()));
            *shown = Some((path, rect));
        }
        None => state.loading = true,
    }
}

fn open_in_editor(terminal: &mut Term, path: &Path) -> io::Result<()> {
    let editor = std::env::var_os("EDITOR")
        .or_else(|| std::env::var_os("VISUAL"))
        .unwrap_or_else(|| OsString::from("hx"));

    leave()?;
    let status = Command::new(&editor).arg(path).status();
    enter_screen()?;
    terminal.clear()?;

    match status {
        Ok(code) if code.success() => Ok(()),
        Ok(code) => Err(io::Error::other(format!(
            "{}: {code}",
            editor.to_string_lossy()
        ))),
        Err(e) => Err(io::Error::new(
            e.kind(),
            format!("{}: {e}", editor.to_string_lossy()),
        )),
    }
}
