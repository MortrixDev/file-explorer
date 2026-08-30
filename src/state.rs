use std::collections::{BTreeSet, HashMap};
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use crate::edit::Edit;
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
    pub message: Option<Message>,
    pub graphics: bool,
    pub loading: bool,
    pub stripe: Option<theme::Rgb>,
    pub pending: Option<PathBuf>,
    pub marks: BTreeSet<PathBuf>,
    pub mode: Mode,
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
            message: None,
            graphics: kitty::supported(),
            loading: false,
            stripe: None,
            pending: None,
            marks: BTreeSet::new(),
            mode: Mode::Normal,
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

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.left.selected().map(|entry| self.left.path_of(entry))
    }

    pub fn say(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: false,
        });
    }

    pub fn fail(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: true,
        });
    }

    pub fn rekey(&mut self, old: &Path, new: &Path) {
        let moved: Vec<PathBuf> = self
            .memory
            .keys()
            .filter(|key| key.starts_with(old))
            .cloned()
            .collect();

        for key in moved {
            let Ok(rest) = key.strip_prefix(old) else {
                continue;
            };
            let target = new.join(rest);
            if let Some(name) = self.memory.remove(&key) {
                self.memory.insert(target, name);
            }
        }
    }

    pub fn toggle_mark(&mut self) {
        let Some(path) = self.selected_path() else {
            return;
        };
        if !self.marks.remove(&path) {
            self.marks.insert(path);
        }
    }

    pub fn reload(&mut self, focus: Option<OsString>) {
        self.remember();
        if let Some(name) = focus {
            self.memory.insert(self.cwd.clone(), name);
        }

        let mut listing = match Listing::load(&self.cwd, &self.filter) {
            Ok(listing) => listing,
            Err(e) => {
                self.fail(format!("{}: {e}", self.cwd.display()));
                return;
            }
        };
        if let Some(name) = self.memory.get(&self.cwd) {
            listing.focus(name);
        }

        self.left = listing;
        self.right = Content::Idle;
        self.right_of = None;
        self.pending = None;
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
                self.fail(format!("{}: {e}", dir.display()));
                return false;
            }
        };

        self.remember();
        if let Some(name) = self.memory.get(&dir) {
            listing.focus(name);
        }
        self.cwd = dir;
        self.left = listing;
        self.message = None;
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

        if let (Content::Dir(_), Some(path)) = (&self.right, &self.right_of) {
            let path = path.clone();
            self.previews.request(path.clone(), true, self.filter);
            self.pending = Some(path);
        }
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

pub struct Message {
    pub text: String,
    pub error: bool,
}

pub enum Mode {
    Normal,
    Rename { edit: Edit },
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
