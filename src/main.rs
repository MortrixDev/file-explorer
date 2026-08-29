use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use file_explorer::kitty;
use file_explorer::preview::Preview;
use file_explorer::state::{Content, State};
use file_explorer::tui;
use ratatui::prelude::*;

type Term = Terminal<CrosstermBackend<io::Stdout>>;

const USAGE: &str = "\
usage: file-explorer [DIR] [--cwd-file PATH]
       file-explorer init <bash|zsh|fish|nu>

  DIR              directory to open (default: current)
  --cwd-file PATH  on exit, write the final directory here
  init SHELL       print a shell function that cds where you left off
";

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

struct Args {
    dir: Option<PathBuf>,
    cwd_file: Option<PathBuf>,
}

fn parse_args() -> io::Result<Args> {
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
            _ => parsed.dir = Some(PathBuf::from(arg)),
        }
    }
    Ok(parsed)
}

fn main() -> io::Result<()> {
    let args = parse_args()?;
    let start = match args.dir {
        Some(dir) => std::fs::canonicalize(dir)?,
        None => std::env::current_dir()?,
    };
    let mut state = State::new(start)?;

    install_panic_hook();
    let mut terminal = enter()?;
    let result = run(&mut terminal, &mut state);
    leave()?;

    if let Some(path) = &args.cwd_file {
        std::fs::write(path, state.cwd.as_os_str().as_bytes())?;
    }
    result
}

fn run(terminal: &mut Term, state: &mut State) -> io::Result<()> {
    let mut loader = kitty::Loader::new();
    let mut shown: Option<(PathBuf, Rect)> = None;

    loop {
        let size = terminal.size()?;
        let area = Rect::new(0, 0, size.width, size.height);
        state.left.scroll_into_view(tui::list_height(area));
        state.sync_right();

        terminal.draw(|frame| tui::draw(frame, state))?;

        loader.poll();
        sync_image(state, &mut loader, &mut shown, tui::preview_area(area));

        let timeout = if loader.busy() { 20 } else { 250 };
        if !event::poll(Duration::from_millis(timeout))? {
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
            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
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
        (Content::File(Preview::Image { .. }), Some(path)) if state.graphics => {
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
    let Some(cell) = kitty::cell_size() else { return };

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
        Ok(_) => Ok(()),
        Err(e) => Err(io::Error::new(
            e.kind(),
            format!("{}: {e}", editor.to_string_lossy()),
        )),
    }
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = leave();
        previous(info);
    }));
}

fn enter() -> io::Result<Term> {
    enter_screen()?;
    Terminal::new(CrosstermBackend::new(io::stdout()))
}

fn enter_screen() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)
}

fn leave() -> io::Result<()> {
    let _ = kitty::clear(&mut io::stdout());
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}
