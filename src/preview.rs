use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

const BYTE_LIMIT: u64 = 128 * 1024;
const HEAD: u64 = 64;
const LINE_LIMIT: usize = 2000;
const COL_LIMIT: usize = 1000;
const TAB: &str = "    ";

pub enum Preview {
    Text { lines: Vec<String>, truncated: bool },
    Binary { size: u64 },
    Image { width: u32, height: u32 },
}

pub fn load(path: &Path) -> io::Result<Preview> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();

    let clipped = size > BYTE_LIMIT;
    let mut bytes = Vec::new();
    file.by_ref().take(HEAD).read_to_end(&mut bytes)?;

    if image::guess_format(&bytes).is_ok()
        && let Ok((width, height)) = image::image_dimensions(path)
    {
        return Ok(Preview::Image { width, height });
    }

    let rest = BYTE_LIMIT - bytes.len() as u64;
    file.by_ref().take(rest).read_to_end(&mut bytes)?;

    if bytes.contains(&0) || (clipped && tail_is_binary(&mut file, size)) {
        return Ok(Preview::Binary { size });
    }

    let text = String::from_utf8_lossy(&bytes);
    let mut rest = text.lines();
    let mut lines: Vec<String> = rest.by_ref().take(LINE_LIMIT).map(clip).collect();
    let truncated = clipped || rest.next().is_some();

    if clipped && lines.len() > 1 && !bytes.ends_with(b"\n") {
        lines.pop();
    }

    Ok(Preview::Text { lines, truncated })
}

fn tail_is_binary(file: &mut File, size: u64) -> bool {
    let mut tail = [0u8; 4096];
    let start = size.saturating_sub(tail.len() as u64);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return false;
    }
    match file.read(&mut tail) {
        Ok(n) => tail[..n].contains(&0),
        Err(_) => false,
    }
}

fn clip(line: &str) -> String {
    let expanded = if line.contains('\t') {
        line.replace('\t', TAB)
    } else {
        line.to_string()
    };
    match expanded.char_indices().nth(COL_LIMIT) {
        Some((i, _)) => expanded[..i].to_string(),
        None => expanded,
    }
}
