use std::io::{self, Write};
use std::time::{Duration, Instant};

pub type Rgb = (u8, u8, u8);

const TIMEOUT: Duration = Duration::from_millis(100);
const LIMIT: usize = 64;
const NUDGE: i16 = 10;
const PREFIX: &[u8] = b"\x1b]11;";

pub fn stripe() -> Option<Rgb> {
    let setting = std::env::var("FEX_STRIPE").unwrap_or_default();
    match setting.as_str() {
        "off" => None,
        "" => derive(NUDGE, background()?),
        spec => match spec.parse::<i16>() {
            Ok(nudge) => derive(nudge, background()?),
            Err(_) => hex(spec),
        },
    }
}

pub fn recheck() -> Option<Option<Rgb>> {
    let nudge = match std::env::var("FEX_STRIPE").unwrap_or_default().as_str() {
        "" => NUDGE,
        spec => spec.parse::<i16>().ok()?,
    };
    Some(derive(nudge, background()?))
}

fn derive(nudge: i16, found: Rgb) -> Option<Rgb> {
    let (r, g, b) = found;
    let luma = (2126 * r as u32 + 7152 * g as u32 + 722 * b as u32) / 10000;
    let delta = if luma < 128 { nudge } else { -nudge };
    Some((shift(r, delta), shift(g, delta), shift(b, delta)))
}

fn hex(spec: &str) -> Option<Rgb> {
    let digits = spec.strip_prefix('#').unwrap_or(spec);
    if digits.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    Some((
        (value >> 16) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    ))
}

pub fn probe() -> String {
    match background() {
        Some(found) => format!("background rgb{found:?}\nstripe {:?}", derive(NUDGE, found)),
        None => "background: no OSC 11 reply, striping disabled".to_string(),
    }
}

fn shift(channel: u8, delta: i16) -> u8 {
    (channel as i16 + delta).clamp(0, 255) as u8
}

fn background() -> Option<(u8, u8, u8)> {
    let mut out = io::stdout();
    out.write_all(b"\x1b]11;?\x1b\\").ok()?;
    out.flush().ok()?;

    let deadline = Instant::now() + TIMEOUT;
    let mut reply = Vec::new();

    while reply.len() < LIMIT {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || !readable(left.as_millis() as i32) {
            return None;
        }
        reply.push(read_byte()?);
        if reply.len() <= PREFIX.len() && reply[..] != PREFIX[..reply.len()] {
            return None;
        }
        if reply.ends_with(b"\x07") || reply.ends_with(b"\x1b\\") {
            return parse(&reply);
        }
    }
    None
}

fn read_byte() -> Option<u8> {
    let mut byte = 0u8;
    let got = unsafe {
        libc::read(
            libc::STDIN_FILENO,
            (&raw mut byte).cast::<libc::c_void>(),
            1,
        )
    };
    (got == 1).then_some(byte)
}

fn readable(millis: i32) -> bool {
    let mut fd = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };
    unsafe { libc::poll(&mut fd, 1, millis) > 0 }
}

fn parse(reply: &[u8]) -> Option<(u8, u8, u8)> {
    let text = std::str::from_utf8(reply).ok()?;
    let mut parts = text.split("rgb:").nth(1)?.split('/');
    let r = component(parts.next()?)?;
    let g = component(parts.next()?)?;
    let b = component(parts.next()?)?;
    Some((r, g, b))
}

fn component(part: &str) -> Option<u8> {
    let hex: String = part.chars().take_while(char::is_ascii_hexdigit).collect();
    if hex.is_empty() || hex.len() > 4 {
        return None;
    }
    let value = u32::from_str_radix(&hex, 16).ok()?;
    let max = (1u32 << (4 * hex.len())) - 1;
    Some((value * 255 / max) as u8)
}
