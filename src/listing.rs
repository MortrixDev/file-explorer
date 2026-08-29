use std::{
    cmp::Ordering,
    ffi::{OsStr, OsString},
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

const SCROLLOFF: usize = 3;

pub struct Listing {
    pub dir: PathBuf,
    all: Vec<Entry>,
    view: Vec<usize>,
    pub cursor: usize,
    pub top: usize,
}

impl Listing {
    pub fn load(dir: &Path, f: &Filter) -> io::Result<Listing> {
        let mut all = Vec::new();

        for item in fs::read_dir(dir)? {
            let Ok(entry) = item else { continue };
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name();
            let kind = if file_type.is_symlink() {
                match fs::metadata(dir.join(&name)) {
                    Ok(target) => kind_of(target.file_type()),
                    Err(_) => Kind::Other,
                }
            } else {
                kind_of(file_type)
            };
            all.push(Entry {
                name,
                kind,
                meta: None,
            });
        }

        let mut listing = Listing {
            dir: dir.to_path_buf(),
            all,
            view: Vec::new(),
            cursor: 0,
            top: 0,
        };
        listing.refilter(f);
        listing.cursor_first();
        Ok(listing)
    }

    pub fn refilter(&mut self, f: &Filter) {
        let mut view: Vec<usize> = (0..self.all.len())
            .filter(|&i| f.show_hidden || !self.all[i].is_hidden())
            .collect();

        view.sort_by(|&x, &y| {
            let (a, b) = (&self.all[x], &self.all[y]);

            let by_name = natural_cmp(a.name.as_bytes(), b.name.as_bytes());
            let by_name = if f.reverse {
                by_name.reverse()
            } else {
                by_name
            };

            if f.dirs_first {
                b.is_dir().cmp(&a.is_dir()).then(by_name)
            } else {
                by_name
            }
        });

        self.view = view;

        if self.cursor_row().is_none() {
            self.cursor = self.view.first().copied().unwrap_or(0);
        }
    }

    pub fn selected(&self) -> Option<&Entry> {
        if self.view.is_empty() {
            return None;
        }
        self.all.get(self.cursor)
    }

    pub fn cursor_row(&self) -> Option<usize> {
        self.view.iter().position(|&i| i == self.cursor)
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.view.iter().map(|&i| &self.all[i])
    }

    pub fn is_empty(&self) -> bool {
        self.view.is_empty()
    }

    pub fn len(&self) -> usize {
        self.view.len()
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let Some(row) = self.cursor_row() else { return };
        let last = self.view.len().saturating_sub(1) as isize;
        let next = (row as isize).saturating_add(delta).clamp(0, last);
        self.cursor = self.view[next as usize];
    }

    pub fn cursor_first(&mut self) {
        if let Some(&i) = self.view.first() {
            self.cursor = i;
        }
    }

    pub fn cursor_last(&mut self) {
        if let Some(&i) = self.view.last() {
            self.cursor = i;
        }
    }

    pub fn scroll_into_view(&mut self, height: usize) {
        let Some(row) = self.cursor_row() else {
            self.top = 0;
            return;
        };
        if height == 0 {
            return;
        }

        let margin = SCROLLOFF.min(height.saturating_sub(1) / 2);
        let lo = row.saturating_sub(margin);
        let hi = (row + margin).min(self.view.len().saturating_sub(1));

        if lo < self.top {
            self.top = lo;
        }
        if hi >= self.top + height {
            self.top = hi + 1 - height;
        }
        self.top = self.top.min(self.view.len().saturating_sub(height));
    }

    pub fn focus(&mut self, name: &OsStr) -> bool {
        match self.index_of(name) {
            Some(i) => {
                self.cursor = i;
                true
            }
            None => false,
        }
    }

    fn index_of(&self, name: &OsStr) -> Option<usize> {
        self.view
            .iter()
            .copied()
            .find(|&i| self.all[i].name == name)
    }

    pub fn path_of(&self, e: &Entry) -> PathBuf {
        self.dir.join(&e.name)
    }
}

fn natural_cmp(a: &[u8], b: &[u8]) -> Ordering {
    let (mut i, mut j) = (0, 0);

    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (si, sj) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }

            // Strip leading zeros, so longer number wins.
            let na = strip_zeros(&a[si..i]);
            let nb = strip_zeros(&b[sj..j]);
            match na.len().cmp(&nb.len()).then_with(|| na.cmp(nb)) {
                Ordering::Equal => {}
                o => return o,
            }
        } else {
            match a[i].to_ascii_lowercase().cmp(&b[j].to_ascii_lowercase()) {
                Ordering::Equal => {}
                o => return o,
            }
            i += 1;
            j += 1;
        }
    }

    (a.len() - i).cmp(&(b.len() - j)).then_with(|| a.cmp(b))
}

fn strip_zeros(s: &[u8]) -> &[u8] {
    s.iter().position(|&c| c != b'0').map_or(&[], |p| &s[p..])
}

#[derive(Clone, Copy)]
pub struct Filter {
    pub show_hidden: bool,
    pub dirs_first: bool,
    pub reverse: bool,
}

pub struct Entry {
    pub name: OsString,
    pub kind: Kind,
    pub meta: Option<Meta>,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Dir
    }

    pub fn is_file(&self) -> bool {
        self.kind == Kind::File
    }

    pub fn is_hidden(&self) -> bool {
        self.name.as_bytes().starts_with(b".")
    }

    pub fn display(&self) -> std::borrow::Cow<'_, str> {
        self.name.to_string_lossy()
    }
}

fn kind_of(file_type: fs::FileType) -> Kind {
    if file_type.is_dir() {
        Kind::Dir
    } else if file_type.is_file() {
        Kind::File
    } else {
        Kind::Other
    }
}

#[derive(PartialEq)]
pub enum Kind {
    Dir,
    File,
    Other,
}

pub struct Meta {}
