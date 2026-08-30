use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::prelude::*;

use crate::commands;
use crate::kitty;
use crate::ops::Op;
use crate::preview::Preview;
use crate::state::{Content, Mode, State};
use crate::term::{self, Term};
use crate::theme;
use crate::tui;

const THEME_INTERVAL: Duration = Duration::from_secs(2);
const SETTLE: Duration = Duration::from_millis(120);

pub enum Exit {
    Cd,
    Stay,
}

pub fn run(terminal: &mut Term, state: &mut State) -> io::Result<Exit> {
    let mut loader = kitty::Loader::new();
    let mut shown: Option<(PathBuf, Rect)> = None;
    let mut target: Option<PathBuf> = None;
    let mut moved = Instant::now();
    let mut bar = false;
    let mut watch_theme = true;
    let mut checked = Instant::now();

    loop {
        let size = terminal.size()?;
        let area = Rect::new(0, 0, size.width, size.height);
        state.left.scroll_into_view(tui::list_height(area));
        state.sync_right();
        state.poll_previews();

        let renaming = matches!(state.mode, Mode::Rename { .. });
        if renaming != bar {
            term::cursor_shape(renaming);
            bar = renaming;
        }

        terminal.draw(|frame| tui::draw(frame, state))?;

        loader.poll();

        let wanted = image_target(state);
        if wanted != target {
            target = wanted;
            moved = Instant::now();
        }
        let settled = target.is_none() || moved.elapsed() >= SETTLE;
        let want = match (&target, settled) {
            (Some(path), true) => Some((path.clone(), tui::preview_area(area))),
            _ => None,
        };
        sync_image(state, &mut loader, &mut shown, want);

        let timeout = if loader.busy() || state.busy() || !settled {
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

        if renaming {
            commands::rename_key(state, key);
            continue;
        }

        state.message = None;

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
                    if let Err(e) = commands::open_in_editor(terminal, &path) {
                        state.fail(e.to_string());
                    }
                    state.right_of = None;
                    shown = None;
                }
            }
            KeyCode::Char(' ') => state.toggle_mark(),
            KeyCode::Char('m') => commands::transfer(state, Op::Move),
            KeyCode::Char('c') => commands::transfer(state, Op::Copy),
            KeyCode::Char('r') => commands::start_rename(state),
            KeyCode::Char('o') => commands::open_selected(state),
            KeyCode::Enter => match state.selected_file() {
                Some(_) => commands::open_selected(state),
                None => state.enter(),
            },
            KeyCode::Char('l') | KeyCode::Right => state.enter(),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => state.leave(),
            _ => {}
        }
    }
}

fn image_target(state: &State) -> Option<PathBuf> {
    match (&state.right, &state.right_of) {
        (Content::File(Preview::Image { .. } | Preview::Pdf { .. }), Some(path))
            if state.graphics && !state.busy() =>
        {
            Some(path.clone())
        }
        _ => None,
    }
}

fn sync_image(
    state: &mut State,
    loader: &mut kitty::Loader,
    shown: &mut Option<(PathBuf, Rect)>,
    want: Option<(PathBuf, Rect)>,
) {
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

    match loader.get(&key) {
        Some(entry) => {
            match &entry.result {
                Ok(_) => {
                    if let Err(e) = kitty::place(&mut out, &entry, rect.x, rect.y) {
                        state.fail(format!("{}: {e}", path.display()));
                    }
                }
                Err(e) => state.fail(format!("{}: {e}", path.display())),
            }
            *shown = Some((path, rect));
        }
        None => state.loading = true,
    }
}
