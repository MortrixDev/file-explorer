use std::time::SystemTime;

use jiff::{Timestamp, tz::TimeZone};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

const SIZE_W: usize = 6;
const DATE_W: usize = 16;
const MIN_NAME: usize = 12;
const GAP: usize = 3;

use crate::listing::Listing;
use crate::preview::Preview;
use crate::state::{Content, State};

fn stripe_color(stripe: Option<crate::theme::Rgb>) -> Option<Color> {
    stripe.map(|(r, g, b)| Color::Rgb(r, g, b))
}

pub struct Panes {
    pub left: Rect,
    pub right: Rect,
    pub status: Rect,
}

pub fn layout(area: Rect) -> Panes {
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let cols =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).split(rows[0]);
    Panes {
        left: cols[0],
        right: cols[1],
        status: rows[1],
    }
}

pub fn preview_area(area: Rect) -> Rect {
    let right = layout(area).right;
    Rect::new(
        right.x + 1,
        right.y + 1,
        right.width.saturating_sub(2),
        right.height.saturating_sub(2),
    )
}

pub fn list_height(area: Rect) -> usize {
    layout(area).left.height.saturating_sub(2) as usize
}

pub fn draw(frame: &mut Frame, state: &State) {
    let panes = layout(frame.area());
    draw_listing(frame, state, panes.left);
    draw_right(frame, state, panes.right);
    draw_status(frame, state, panes.status);
}

fn draw_listing(frame: &mut Frame, state: &State, area: Rect) {
    let cwd = sanitize(&state.cwd.to_string_lossy());
    let title = if cwd.ends_with('/') {
        format!(" {cwd} ")
    } else {
        format!(" {cwd}/ ")
    };
    let block = Block::default().borders(Borders::ALL).title(heading(title));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = entry_lines(
        &state.left,
        inner.height as usize,
        state.left.cursor_row(),
        inner.width,
        stripe_color(state.stripe),
    );

    if lines.is_empty() {
        frame.render_widget(dim("  (empty)"), inner);
    } else {
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

fn entry_lines(
    listing: &Listing,
    height: usize,
    cursor: Option<usize>,
    width: u16,
    stripe: Option<Color>,
) -> Vec<Line<'static>> {
    let cols = columns_for(width);
    rows(listing, height)
        .map(|(row, entry)| {
            let on_cursor = Some(row) == cursor;

            let mut row_style = Style::default();
            if on_cursor {
                row_style = row_style.add_modifier(Modifier::REVERSED);
            } else if row % 2 == 1
                && let Some(color) = stripe
            {
                row_style = row_style.bg(color);
            }

            let mut name_style = Style::default();
            if entry.is_dir() {
                name_style = name_style.fg(Color::Blue).add_modifier(Modifier::BOLD);
            }
            if entry.is_exec() {
                name_style = name_style.fg(Color::Green);
            }
            if entry.is_hidden() {
                name_style = name_style.add_modifier(Modifier::DIM);
            }

            let mut meta_style = Style::default();
            if on_cursor {
                if let Some(fg) = name_style.fg {
                    meta_style = meta_style.fg(fg);
                }
            } else {
                meta_style = meta_style.add_modifier(Modifier::DIM);
            }

            let suffix = if entry.is_dir() { "/" } else { "" };
            let name = format!("{}{suffix}", sanitize(&entry.display()));
            let (name_cell, meta_cell) = columns(&name, entry, &cols, width);

            Line::from(vec![
                Span::styled(name_cell, name_style),
                Span::styled(meta_cell, meta_style),
            ])
            .style(row_style)
        })
        .collect()
}

struct Columns {
    name: usize,
    size: bool,
    date: bool,
}

fn columns_for(width: u16) -> Columns {
    let width = width as usize;
    let size_cost = GAP + SIZE_W;
    let date_cost = GAP + DATE_W;
    let fixed = 1 + 1;

    if width >= fixed + MIN_NAME + size_cost + date_cost {
        Columns {
            name: width - (fixed + size_cost + date_cost),
            size: true,
            date: true,
        }
    } else if width >= fixed + MIN_NAME + size_cost {
        Columns {
            name: width - (fixed + size_cost),
            size: true,
            date: false,
        }
    } else {
        Columns {
            name: width.saturating_sub(1),
            size: false,
            date: false,
        }
    }
}

fn columns(
    name: &str,
    entry: &crate::listing::Entry,
    cols: &Columns,
    width: u16,
) -> (String, String) {
    let name_cell = format!(" {:<width$}", clip(name, cols.name), width = cols.name);

    let mut meta = String::new();
    if cols.size {
        let text = entry.size().map_or_else(String::new, human);
        meta.push_str(&format!("{blank:GAP$}{text:>SIZE_W$}", blank = ""));
    }
    if cols.date {
        let text = entry.modified().map_or_else(String::new, stamp);
        meta.push_str(&format!("{blank:GAP$}{text:>DATE_W$}", blank = ""));
    }

    let filled = name_cell.width() + meta.width();
    let gap = (width as usize).saturating_sub(filled);
    (name_cell, format!("{meta}{blank:gap$}", blank = ""))
}

fn human(size: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = size as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{size}B")
    } else if value < 10.0 {
        format!("{value:.1}{}", UNITS[unit])
    } else {
        format!("{value:.0}{}", UNITS[unit])
    }
}

fn stamp(time: SystemTime) -> String {
    let Ok(moment) = Timestamp::try_from(time) else {
        return String::new();
    };
    moment
        .to_zoned(TimeZone::system())
        .strftime("%Y-%m-%d %H:%M")
        .to_string()
}

fn clip(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

fn draw_right(frame: &mut Frame, state: &State, area: Rect) {
    let title = match &state.right_of {
        Some(path) => match path.file_name() {
            Some(name) => {
                let suffix = if matches!(state.right, Content::Dir(_)) {
                    "/"
                } else {
                    ""
                };
                format!(" {}{suffix} ", sanitize(&name.to_string_lossy()))
            }
            None => String::new(),
        },
        None => String::new(),
    };
    let block = Block::default().borders(Borders::ALL).title(heading(title));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match &state.right {
        Content::File(Preview::Text { lines, truncated }) => {
            draw_text(frame, lines, *truncated, inner)
        }
        Content::File(Preview::Binary { size }) => {
            frame.render_widget(dim(&format!("  binary, {size} bytes")), inner)
        }
        Content::File(Preview::Pdf { size }) => {
            if !state.graphics {
                frame.render_widget(dim(&format!("  pdf, {size} bytes")), inner)
            } else if state.loading {
                frame.render_widget(dim("  rendering…"), inner)
            }
        }
        Content::File(Preview::Image { width, height }) => {
            if !state.graphics {
                frame.render_widget(dim(&format!("  image, {width}x{height}")), inner)
            } else if state.loading {
                frame.render_widget(dim("  loading…"), inner)
            }
        }
        Content::Error(e) => frame.render_widget(
            Paragraph::new(Line::styled(
                format!("  {}", sanitize(e)),
                Style::default().fg(Color::Red),
            )),
            inner,
        ),
        Content::Dir(listing) => draw_dir(frame, listing, inner, stripe_color(state.stripe)),
        Content::Idle => frame.render_widget(dim("  no preview"), inner),
    }
}

fn draw_dir(frame: &mut Frame, listing: &Listing, area: Rect, stripe: Option<Color>) {
    let height = area.height as usize;
    let mut lines = entry_lines(listing, height, None, area.width, stripe);

    if lines.is_empty() {
        frame.render_widget(dim("  (empty)"), area);
        return;
    }
    if listing.len() > height {
        lines.pop();
        lines.push(ellipsis());
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_text(frame: &mut Frame, lines: &[String], truncated: bool, area: Rect) {
    if lines.is_empty() {
        frame.render_widget(dim("  (empty)"), area);
        return;
    }

    let height = area.height as usize;
    let mut shown: Vec<Line> = lines
        .iter()
        .take(height)
        .map(|line| Line::raw(sanitize(line)))
        .collect();

    if (truncated || lines.len() > height) && shown.len() == height {
        shown.pop();
        shown.push(ellipsis());
    }

    frame.render_widget(Paragraph::new(shown), area);
}

fn heading(title: String) -> Line<'static> {
    Line::styled(title, Style::default().fg(Color::Yellow))
}

fn ellipsis() -> Line<'static> {
    Line::styled("…", Style::default().add_modifier(Modifier::DIM))
}

fn draw_status(frame: &mut Frame, state: &State, area: Rect) {
    if let Some(error) = &state.error {
        let line = Line::styled(
            format!(" {}", sanitize(error)),
            Style::default().fg(Color::Red),
        );
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    let position = match state.left.cursor_row() {
        Some(row) => format!("{}/{}", row + 1, state.left.len()),
        None => "0/0".to_string(),
    };
    let hidden = if state.filter.show_hidden { " ." } else { "" };
    frame.render_widget(dim(&format!(" {position}{hidden}")), area);
}

fn rows(listing: &Listing, height: usize) -> impl Iterator<Item = (usize, &crate::listing::Entry)> {
    listing.entries().enumerate().skip(listing.top).take(height)
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .collect()
}

fn dim(s: &str) -> Paragraph<'static> {
    Paragraph::new(Line::styled(
        s.to_string(),
        Style::default().add_modifier(Modifier::DIM),
    ))
}
