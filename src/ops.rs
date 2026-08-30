use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq)]
pub enum Op {
    Move,
    Copy,
}

impl Op {
    fn verb(self) -> &'static str {
        match self {
            Op::Move => "moved",
            Op::Copy => "copied",
        }
    }
}

pub struct Report {
    pub done: Vec<(PathBuf, PathBuf)>,
    pub skipped: usize,
    pub failed: Vec<String>,
}

impl Report {
    pub fn message(&self, op: Op) -> String {
        let mut parts = vec![format!("{} {}", self.done.len(), op.verb())];
        if self.skipped > 0 {
            parts.push(format!("{} skipped (exists)", self.skipped));
        }
        parts.extend(self.failed.iter().cloned());
        parts.join(", ")
    }
}

pub fn transfer(marks: &BTreeSet<PathBuf>, dest: &Path, op: Op) -> Report {
    let mut report = Report {
        done: Vec::new(),
        skipped: 0,
        failed: Vec::new(),
    };

    for src in marks {
        let Some(name) = src.file_name() else {
            report.failed.push(format!("{}: no name", src.display()));
            continue;
        };
        let target = dest.join(name);

        if target == *src {
            report.skipped += 1;
            continue;
        }
        if dest.starts_with(src) {
            report
                .failed
                .push(format!("{}: into itself", name.to_string_lossy()));
            continue;
        }
        if fs::symlink_metadata(&target).is_ok() {
            report.skipped += 1;
            continue;
        }

        let result = match op {
            Op::Move => move_one(src, &target),
            Op::Copy => copy_tree(src, &target),
        };
        match result {
            Ok(()) => report.done.push((src.clone(), target)),
            Err(e) => report
                .failed
                .push(format!("{}: {e}", name.to_string_lossy())),
        }
    }

    report
}

pub fn rename(path: &Path, name: &str) -> io::Result<PathBuf> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return Err(io::Error::other("invalid name"));
    }
    let Some(parent) = path.parent() else {
        return Err(io::Error::other("no parent directory"));
    };

    let target = parent.join(name);
    if target == path {
        return Ok(target);
    }
    if fs::symlink_metadata(&target).is_ok() {
        return Err(io::Error::other(format!("{name}: exists")));
    }

    fs::rename(path, &target)?;
    Ok(target)
}

fn move_one(src: &Path, dst: &Path) -> io::Result<()> {
    match fs::rename(src, dst) {
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            if let Err(e) = copy_tree(src, dst) {
                let _ = remove(dst);
                return Err(e);
            }
            remove(src)
        }
        other => other,
    }
}

fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    let kind = fs::symlink_metadata(src)?.file_type();

    if kind.is_symlink() {
        symlink(fs::read_link(src)?, dst)
    } else if kind.is_dir() {
        fs::create_dir(dst)?;
        for item in fs::read_dir(src)? {
            let item = item?;
            copy_tree(&item.path(), &dst.join(item.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dst).map(|_| ())
    }
}

fn remove(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}
