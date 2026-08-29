use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

const BYTE_LIMIT: u64 = 128 * 1024;
const LINE_LIMIT: usize = 2000;
const COL_LIMIT: usize = 1000;
const TAB: &str = "    ";

pub enum Preview {
    Text { lines: Vec<String>, truncated: bool },
    Binary { size: u64 },
    Image { width: u32, height: u32 },
}

pub fn load(path: &Path) -> io::Result<Preview> {
    let file = File::open(path)?;
    let size = file.metadata()?.len();

    let mut bytes = Vec::new();
    file.take(BYTE_LIMIT).read_to_end(&mut bytes)?;

    if image::guess_format(&bytes).is_ok()
        && let Ok((width, height)) = image::image_dimensions(path)
    {
        return Ok(Preview::Image { width, height });
    }

    if bytes.contains(&0) {
        return Ok(Preview::Binary { size });
    }

    let text = String::from_utf8_lossy(&bytes);
    let mut rest = text.lines();
    let lines: Vec<String> = rest.by_ref().take(LINE_LIMIT).map(clip).collect();
    let truncated = size > BYTE_LIMIT || rest.next().is_some();

    Ok(Preview::Text { lines, truncated })
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
