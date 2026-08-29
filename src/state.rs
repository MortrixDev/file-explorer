use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use crate::kitty;
use crate::listing::{Filter, Listing};
use crate::preview::{self, Preview};
use crate::theme;

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
    pub stripe: Option<theme::Rgb>,
    pub pending: Option<PathBuf>,
    previews: Previews,
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
            stripe: None,
            pending: None,
            previews: Previews::new(),
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
        if self.goto(parent)
            && let Some(name) = from
        {
            self.left.focus(&name);
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
        if target == self.right_of || target == self.pending {
            return;
        }
        self.error = None;

        match target {
            None => {
                self.right = Content::Idle;
                self.right_of = None;
                self.pending = None;
            }
            Some(path) => {
                self.previews.request(path.clone(), is_dir, self.filter);
                self.pending = Some(path);
            }
        }
    }

    pub fn poll_previews(&mut self) {
        while let Some((path, content)) = self.previews.take() {
            if self.pending.as_ref() == Some(&path) {
                self.right = content;
                self.right_of = Some(path);
                self.pending = None;
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
}

pub enum Content {
    Idle,
    Dir(Listing),
    File(Preview),
    Error(String),
}

struct Job {
    path: PathBuf,
    is_dir: bool,
    filter: Filter,
}

pub struct Previews {
    jobs: Sender<Job>,
    done: Receiver<(PathBuf, Content)>,
}

impl Previews {
    fn new() -> Previews {
        let (jobs, job_rx) = mpsc::channel::<Job>();
        let (done_tx, done) = mpsc::channel();

        thread::spawn(move || {
            while let Ok(mut job) = job_rx.recv() {
                loop {
                    match job_rx.try_recv() {
                        Ok(next) => job = next,
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return,
                    }
                }
                if done_tx.send((job.path.clone(), build(&job))).is_err() {
                    return;
                }
            }
        });

        Previews { jobs, done }
    }

    fn request(&self, path: PathBuf, is_dir: bool, filter: Filter) {
        let _ = self.jobs.send(Job {
            path,
            is_dir,
            filter,
        });
    }

    fn take(&self) -> Option<(PathBuf, Content)> {
        self.done.try_recv().ok()
    }
}

fn build(job: &Job) -> Content {
    if job.is_dir {
        match Listing::load(&job.path, &job.filter) {
            Ok(listing) => Content::Dir(listing),
            Err(e) => Content::Error(format!("{e}")),
        }
    } else {
        match preview::load(&job.path) {
            Ok(preview) => Content::File(preview),
            Err(e) => Content::Error(format!("{e}")),
        }
    }
}
