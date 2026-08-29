use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use base64::Engine;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{RenderCache, RenderSettings, render as render_page};

const MIN_SCALE: f32 = 0.01;
const CHUNK: usize = 4096;
const ID: u32 = 1;
const CACHE: usize = 32;
const UPSCALE_SHARP: u32 = 2;

pub type Key = (PathBuf, u32, u32);

pub struct Image {
    png: Vec<u8>,
}

pub struct Loader {
    jobs: Sender<Key>,
    done: Receiver<(Key, Option<io::Result<Image>>)>,
    cache: HashMap<Key, Rc<io::Result<Image>>>,
    order: VecDeque<Key>,
    inflight: HashSet<Key>,
}

impl Loader {
    pub fn new() -> Loader {
        let (jobs, job_rx) = mpsc::channel::<Key>();
        let (done_tx, done) = mpsc::channel();

        thread::spawn(move || {
            while let Ok(mut key) = job_rx.recv() {
                loop {
                    match job_rx.try_recv() {
                        Ok(next) => {
                            let stale = std::mem::replace(&mut key, next);
                            if done_tx.send((stale, None)).is_err() {
                                return;
                            }
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return,
                    }
                }
                if done_tx.send((key.clone(), Some(render(&key)))).is_err() {
                    return;
                }
            }
        });

        Loader {
            jobs,
            done,
            cache: HashMap::new(),
            order: VecDeque::new(),
            inflight: HashSet::new(),
        }
    }

    pub fn busy(&self) -> bool {
        !self.inflight.is_empty()
    }

    pub fn poll(&mut self) -> bool {
        let mut arrived = false;
        loop {
            match self.done.try_recv() {
                Ok((key, image)) => {
                    self.inflight.remove(&key);
                    arrived = true;
                    if let Some(image) = image {
                        self.insert(key, image);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.inflight.clear();
                    break;
                }
            }
        }
        arrived
    }

    pub fn get(&mut self, key: &Key) -> Option<Rc<io::Result<Image>>> {
        if let Some(hit) = self.cache.get(key) {
            return Some(Rc::clone(hit));
        }
        if !self.inflight.contains(key) && self.jobs.send(key.clone()).is_ok() {
            self.inflight.insert(key.clone());
        }
        None
    }

    fn insert(&mut self, key: Key, image: io::Result<Image>) {
        if self.cache.contains_key(&key) {
            return;
        }
        self.cache.insert(key.clone(), Rc::new(image));
        self.order.push_back(key);
        while self.order.len() > CACHE {
            if let Some(old) = self.order.pop_front() {
                self.cache.remove(&old);
            }
        }
    }
}

impl Default for Loader {
    fn default() -> Loader {
        Loader::new()
    }
}

fn render(key: &Key) -> io::Result<Image> {
    let (path, max_w, max_h) = key;
    let image = source(path, *max_w, *max_h)?;
    let image = fit(image, *max_w, *max_h);

    let mut png = io::Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(io::Error::other)?;
    Ok(Image {
        png: png.into_inner(),
    })
}

fn source(path: &Path, max_w: u32, max_h: u32) -> io::Result<image::DynamicImage> {
    if !is_pdf(path) {
        return image::open(path).map_err(io::Error::other);
    }
    let data = std::fs::read(path)?;
    std::panic::catch_unwind(|| rasterize(data, max_w, max_h))
        .unwrap_or_else(|_| Err(io::Error::other("pdf: renderer failed")))
}

fn is_pdf(path: &Path) -> bool {
    let mut head = [0u8; 5];
    match std::fs::File::open(path).and_then(|mut file| file.read(&mut head)) {
        Ok(5) => &head == b"%PDF-",
        _ => false,
    }
}

fn rasterize(data: Vec<u8>, max_w: u32, max_h: u32) -> io::Result<image::DynamicImage> {
    let pdf = Pdf::new(data).map_err(|e| io::Error::other(format!("pdf: {e:?}")))?;
    let pages = pdf.pages();
    let page = pages
        .iter()
        .next()
        .ok_or_else(|| io::Error::other("pdf: no pages"))?;

    let (w, h) = page.render_dimensions();
    if w <= 0.0 || h <= 0.0 {
        return Err(io::Error::other("pdf: empty page"));
    }
    let scale = (max_w as f32 / w).min(max_h as f32 / h).max(MIN_SCALE);

    let settings = RenderSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: WHITE,
        ..Default::default()
    };
    let pixmap = render_page(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &settings,
    );

    let (pw, ph) = (pixmap.width() as u32, pixmap.height() as u32);
    image::RgbaImage::from_raw(pw, ph, pixmap.data_as_u8_slice().to_vec())
        .map(image::DynamicImage::ImageRgba8)
        .ok_or_else(|| io::Error::other("pdf: bad pixmap"))
}

fn fit(image: image::DynamicImage, max_w: u32, max_h: u32) -> image::DynamicImage {
    let (w, h) = (image.width(), image.height());
    if w == 0 || h == 0 {
        return image;
    }
    if w > max_w || h > max_h {
        return image.thumbnail(max_w, max_h);
    }
    let factor = (max_w / w).min(max_h / h);
    if factor >= UPSCALE_SHARP {
        return upscale(&image, factor);
    }
    image.resize(max_w, max_h, image::imageops::FilterType::Triangle)
}

fn upscale(image: &image::DynamicImage, factor: u32) -> image::DynamicImage {
    let src = image.to_rgba8();
    let (w, h) = (src.width() as usize, src.height() as usize);
    let f = factor as usize;
    let stride = w * f * 4;

    let mut out = Vec::with_capacity(stride * h * f);
    for y in 0..h {
        let start = out.len();
        for x in 0..w {
            let px = &src.as_raw()[(y * w + x) * 4..(y * w + x) * 4 + 4];
            for _ in 0..f {
                out.extend_from_slice(px);
            }
        }
        for _ in 1..f {
            out.extend_from_within(start..start + stride);
        }
    }

    match image::RgbaImage::from_raw((w * f) as u32, (h * f) as u32, out) {
        Some(scaled) => image::DynamicImage::ImageRgba8(scaled),
        None => image.clone(),
    }
}

pub struct CellSize {
    pub w: u32,
    pub h: u32,
}

pub fn supported() -> bool {
    if std::env::var_os("KITTY_WINDOW_ID").is_some() {
        return true;
    }
    let names = ["TERM", "TERM_PROGRAM"];
    names.iter().any(|key| match std::env::var(key) {
        Ok(value) => {
            let value = value.to_ascii_lowercase();
            value.contains("kitty") || value.contains("ghostty") || value.contains("wezterm")
        }
        Err(_) => false,
    })
}

pub fn cell_size() -> Option<CellSize> {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &raw mut ws) };
    if rc != 0 || ws.ws_col == 0 || ws.ws_row == 0 || ws.ws_xpixel == 0 || ws.ws_ypixel == 0 {
        return None;
    }
    Some(CellSize {
        w: (ws.ws_xpixel / ws.ws_col) as u32,
        h: (ws.ws_ypixel / ws.ws_row) as u32,
    })
}

pub fn place(out: &mut impl Write, image: &Image, x: u16, y: u16) -> io::Result<()> {
    if !supported() {
        return Ok(());
    }
    let payload = base64::engine::general_purpose::STANDARD.encode(&image.png);

    write!(out, "\x1b[{};{}H", y + 1, x + 1)?;
    let mut chunks = payload.as_bytes().chunks(CHUNK).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        if first {
            write!(out, "\x1b_Ga=T,f=100,i={ID},q=2,C=1,m={more};")?;
            first = false;
        } else {
            write!(out, "\x1b_Gm={more};")?;
        }
        out.write_all(chunk)?;
        out.write_all(b"\x1b\\")?;
    }
    out.flush()
}

pub fn clear(out: &mut impl Write) -> io::Result<()> {
    if !supported() {
        return Ok(());
    }
    write!(out, "\x1b_Ga=d,d=I,i={ID},q=2\x1b\\")?;
    out.flush()
}
