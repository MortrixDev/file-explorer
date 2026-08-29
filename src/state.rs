use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};

use crate::kitty;
use crate::listing::{Filter, Listing};
use crate::preview::{self, Preview};

pub struct State {
    pub cwd: PathBuf,
    pub left: Listing,
    pub right: Content,
    pub right_of: Option<PathBuf>,
    pub filter: Filter,
    pub memory: HashMap<PathBuf, OsString>,
    pub error: Option<String>,
    pub graphics: bool,
    pub loading: bool,
}

impl State {
    pub fn new(cwd: PathBuf) -> io::Result<State> {
        let filter = Filter {
            show_hidden: false,
            dirs_first: true,
            reverse: false,
        };
        let left = Listing::load(&cwd, &filter)?;
        Ok(State {
            cwd,
            left,
            right: Content::Idle,
            right_of: None,
            filter,
            memory: HashMap::new(),
            error: None,
            graphics: kitty::supported(),
            loading: false,
        })
    }

    pub fn enter(&mut self) {
        let target = match self.left.selected() {
            Some(entry) if entry.is_dir() => self.left.path_of(entry),
            _ => return,
        };
        self.goto(target);
    }

    pub fn selected_file(&self) -> Option<PathBuf> {
        match self.left.selected() {
            Some(entry) if entry.is_file() => Some(self.left.path_of(entry)),
            _ => None,
        }
    }

    pub fn leave(&mut self) {
        let Some(parent) = self.cwd.parent().map(Path::to_path_buf) else {
            return;
        };
        let from = self.cwd.file_name().map(OsStr::to_os_string);
        if self.goto(parent) {
            if let Some(name) = from {
                self.left.focus(&name);
            }
        }
    }

    fn goto(&mut self, dir: PathBuf) -> bool {
        let mut listing = match Listing::load(&dir, &self.filter) {
            Ok(listing) => listing,
            Err(e) => {
                self.error = Some(format!("{}: {e}", dir.display()));
                return false;
            }
        };

        self.remember();
        if let Some(name) = self.memory.get(&dir) {
            listing.focus(name);
        }
        self.cwd = dir;
        self.left = listing;
        self.error = None;
        true
    }

    fn remember(&mut self) {
        if let Some(entry) = self.left.selected() {
            self.memory.insert(self.cwd.clone(), entry.name.clone());
        }
    }

    pub fn toggle_hidden(&mut self) {
        self.filter.show_hidden = !self.filter.show_hidden;
        self.left.refilter(&self.filter);
        self.right_of = None;
    }

    pub fn sync_right(&mut self) {
        let (target, is_dir) = match self.left.selected() {
            Some(entry) if entry.is_dir() => (Some(self.left.path_of(entry)), true),
            Some(entry) if entry.is_file() => (Some(self.left.path_of(entry)), false),
            _ => (None, false),
        };
        if target == self.right_of {
            return;
        }

        self.right = match &target {
            Some(path) if is_dir => match Listing::load(path, &self.filter) {
                Ok(listing) => Content::Dir(listing),
                Err(e) => Content::Error(format!("{e}")),
            },
            Some(path) => match preview::load(path) {
                Ok(preview) => Content::File(preview),
                Err(e) => Content::Error(format!("{e}")),
            },
            None => Content::Idle,
        };
        self.right_of = target;
    }
}

pub enum Content {
    Idle,
    Dir(Listing),
    File(Preview),
    Error(String),
}
