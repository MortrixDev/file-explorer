use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

use crossterm::event::KeyEvent;

use crate::edit::{Action, Edit};
use crate::ops::{self, Op};
use crate::state::{Mode, State};
use crate::term::{Term, enter_screen, leave};

pub fn transfer(state: &mut State, op: Op) {
    if state.marks.is_empty() {
        state.say("no marks");
        return;
    }

    let report = ops::transfer(&state.marks, &state.cwd, op);
    let message = report.message(op);

    for (src, dst) in &report.done {
        state.marks.remove(src);
        if op == Op::Move {
            state.rekey(src, dst);
        }
    }

    state.reload(None);
    if report.failed.is_empty() {
        state.say(message);
    } else {
        state.fail(message);
    }
}

pub fn start_rename(state: &mut State) {
    let Some(entry) = state.left.selected() else {
        return;
    };
    state.mode = Mode::Rename {
        edit: Edit::new(entry.name.to_string_lossy().into_owned()),
    };
}

pub fn rename_key(state: &mut State, key: KeyEvent) {
    let action = {
        let Mode::Rename { edit } = &mut state.mode else {
            return;
        };
        edit.apply(key)
    };

    match action {
        Action::Continue => {}
        Action::Cancel => state.mode = Mode::Normal,
        Action::Commit => commit_rename(state),
    }
}

fn commit_rename(state: &mut State) {
    let Mode::Rename { edit } = std::mem::replace(&mut state.mode, Mode::Normal) else {
        return;
    };
    let name = edit.text().trim().to_string();
    let Some(old) = state.selected_path() else {
        return;
    };

    match ops::rename(&old, &name) {
        Ok(new) => {
            if state.marks.remove(&old) {
                state.marks.insert(new.clone());
            }
            state.rekey(&old, &new);
            state.reload(new.file_name().map(OsStr::to_os_string));
            state.say(format!("renamed to {name}"));
        }
        Err(e) => state.fail(format!("{}: {e}", old.display())),
    }
}

pub fn open_selected(state: &mut State) {
    let Some(path) = state.selected_file() else {
        return;
    };
    let spawned = Command::new("xdg-open")
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    if let Err(e) = spawned {
        state.fail(format!("{}: {e}", path.display()));
    }
}

pub fn open_in_editor(terminal: &mut Term, path: &Path) -> io::Result<()> {
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
