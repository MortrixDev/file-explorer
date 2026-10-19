use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::SystemTime;

use jiff::{Timestamp, tz::TimeZone};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

const MIN_FOR_HINTS: u16 = 10;
const HINT_GAP: usize = 2;

const HINTS: &[(&str, &str)] = &[
    ("q", "quit"),
    ("Space", "mark"),
    ("r", "rename"),
    ("m", "move"),
    ("c", "copy"),
    ("e", "edit"),
    ("o", "open"),
    (".", "hidden"),
];

const MARK: &str = " ● ";
const BLANK: &str = "   ";
const MARK_W: usize = 3;
const SIZE_W: usize = 6;
const DATE_W: usize = 16;
const MIN_NAME: usize = 12;
const GAP: usize = 3;

use crate::edit::Edit;
use crate::highlight::{Piece, Tone};
use crate::listing::Listing;
use crate::preview::Preview;
use crate::state::{Content, Mode, State};

fn stripe_color(stripe: Option<crate::theme::Rgb>) -> Option<Color> {
    stripe.map(|(r, g, b)| Color::Rgb(r, g, b))
}

struct Look<'a> {
    stripe: Option<Color>,
    marks: &'a BTreeSet<PathBuf>,
    edit: Option<(usize, &'a Edit)>,
}

fn look(state: &State) -> Look<'_> {
    Look {
        stripe: stripe_color(state.stripe),
        marks: &state.marks,
        edit: None,
    }
}

fn look_editing(state: &State) -> Look<'_> {
    let mut look = look(state);
    if let Mode::Rename { edit } = &state.mode
        && let Some(row) = state.left.cursor_row()
    {
        look.edit = Some((row, edit));
    }
    look
}

struct Rendered {
    lines: Vec<Line<'static>>,
    caret: Option<(u16, u16)>,
}

fn edit_view(edit: &Edit, field: usize) -> (String, usize) {
    let caret = edit.caret();
    let scroll = caret.saturating_sub(field.saturating_sub(1));
    (take_columns(edit.text(), scroll, field), caret - scroll)
}

fn take_columns(text: &str, from: usize, width: usize) -> String {
    let mut out = String::new();
    let mut col = 0;
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if col + w > from + width {
            break;
        }
        if col >= from {
            out.push(ch);
        }
        col += w;
    }
    out
}

pub struct Panes {
    pub left: Rect,
    pub right: Rect,
    pub hints: Rect,
    pub status: Rect,
}

pub fn layout(area: Rect) -> Panes {
    let rows = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(hint_rows(area)),
        Constraint::Length(1),
    ])
    .split(area);
    let cols =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).split(rows[0]);
    Panes {
        left: cols[0],
        right: cols[1],
        hints: rows[1],
        status: rows[2],
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
    draw_hints(frame, state, panes.hints);
    draw_status(frame, state, panes.status);
}

fn draw_hints(frame: &mut Frame, state: &State, area: Rect) {
    if area.height == 0 {
        return;
    }

    let key_style = Style::default().add_modifier(Modifier::REVERSED);
    let label_style = match stripe_color(state.stripe) {
        Some(color) => Style::default().bg(color),
        None => Style::default().add_modifier(Modifier::DIM),
    };

    let (rows, per_row) = hint_layout(area.width);
    let mut lines = Vec::new();

    for row in 0..rows {
        if row > 0 {
            lines.push(Line::default());
        }

        let items = HINTS.iter().skip(row * per_row).take(per_row);
        let mut spans = Vec::new();
        for (index, (key, label)) in items.enumerate() {
            if index > 0 {
                spans.push(Span::raw(" ".repeat(HINT_GAP)));
            }
            spans.push(Span::styled(format!(" {key} "), key_style));
            spans.push(Span::styled(format!(" {label} "), label_style));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines).centered(), area);
}

fn hint_width((key, label): (&str, &str)) -> usize {
    key.width() + label.width() + 4
}

fn hint_row_width(items: &[(&str, &str)]) -> usize {
    let entries: usize = items.iter().copied().map(hint_width).sum();
    entries + HINT_GAP * items.len().saturating_sub(1)
}

fn hint_layout(width: u16) -> (usize, usize) {
    if hint_row_width(HINTS) <= width as usize {
        (1, HINTS.len())
    } else {
        (2, HINTS.len().div_ceil(2))
    }
}

fn hint_rows(area: Rect) -> u16 {
    if area.height < MIN_FOR_HINTS {
        return 0;
    }
    let rows = hint_layout(area.width).0 as u16;
    rows * 2 - 1
}

fn draw_listing(frame: &mut Frame, state: &State, area: Rect) {
    let cwd = sanitize(&state.cwd.to_string_lossy());
    let title = if cwd.ends_with('/') {
        cwd
    } else {
        format!("{cwd}/")
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(heading(&title, area));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rendered = entry_lines(
        &state.left,
        inner.height as usize,
        state.left.cursor_row(),
        inner.width,
        &look_editing(state),
    );

    if rendered.lines.is_empty() {
        frame.render_widget(dim("  (empty)"), inner);
    } else {
        frame.render_widget(Paragraph::new(rendered.lines), inner);
    }

    if let Some((x, y)) = rendered.caret {
        frame.set_cursor_position((inner.x + x, inner.y + y));
    }
}

fn entry_lines(
    listing: &Listing,
    height: usize,
    cursor: Option<usize>,
    width: u16,
    look: &Look,
) -> Rendered {
    let cols = columns_for(width);
    let mut lines = Vec::new();
    let mut caret = None;

    for (row, entry) in rows(listing, height) {
        let on_cursor = Some(row) == cursor;
        let editing = look.edit.and_then(|(at, edit)| (at == row).then_some(edit));

        let mut row_style = Style::default();
        if !on_cursor
            && row % 2 == 1
            && let Some(color) = look.stripe
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
        if editing.is_some() {
            name_style = name_style.add_modifier(Modifier::UNDERLINED);
            meta_style = meta_style.add_modifier(Modifier::DIM);
        } else if on_cursor {
            if let Some(fg) = name_style.fg {
                meta_style = meta_style.fg(fg);
            }
            meta_style = meta_style.add_modifier(Modifier::REVERSED);
            name_style = name_style.add_modifier(Modifier::REVERSED);
        } else {
            meta_style = meta_style.add_modifier(Modifier::DIM);
        }

        let marked = look.marks.contains(&listing.path_of(entry));
        let mark_cell = if marked { MARK } else { BLANK };
        let mark_style = if on_cursor && editing.is_none() {
            Style::default()
                .fg(name_style.fg.unwrap_or(Color::Reset))
                .bg(Color::Red)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(Color::Red)
        };

        let name = match editing {
            Some(edit) => {
                let (text, col) = edit_view(edit, cols.name);
                caret = Some(((MARK_W + col) as u16, lines.len() as u16));
                sanitize(&text)
            }
            None => {
                let suffix = if entry.is_dir() { "/" } else { "" };
                format!("{}{suffix}", sanitize(&entry.display()))
            }
        };
        let (name_cell, meta_cell) = columns(&name, entry, &cols, width);

        lines.push(
            Line::from(vec![
                Span::styled(mark_cell, mark_style),
                Span::styled(name_cell, name_style),
                Span::styled(meta_cell, meta_style),
            ])
            .style(row_style),
        );
    }

    Rendered { lines, caret }
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
    let fixed = MARK_W + 1;

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
    let name_cell = format!("{:<width$}", clip_right(name, cols.name), width = cols.name);

    let mut meta = String::new();
    if cols.size {
        let text = entry.size().map_or_else(String::new, human);
        meta.push_str(&format!("{blank:GAP$}{text:>SIZE_W$}", blank = ""));
    }
    if cols.date {
        let text = entry.modified().map_or_else(String::new, stamp);
        meta.push_str(&format!("{blank:GAP$}{text:>DATE_W$}", blank = ""));
    }

    let filled = MARK_W + name_cell.width() + meta.width();
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

fn clip_left(text: &str, width: usize) -> String {
    let total = text.width();
    if total <= width {
        return text.to_string();
    }
    let budget = width.saturating_sub(1);
    format!("…{}", take_columns(text, total - budget, budget))
}

fn clip_right(text: &str, width: usize) -> String {
    let total = text.width();
    if total <= width {
        return text.to_string();
    }
    let budget = width.saturating_sub(1);
    format!("{}…", take_columns(text, 0, budget))
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
    let block = Block::default()
        .borders(Borders::ALL)
        .title(heading(&title, area));
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
        Content::Dir(listing) => draw_dir(frame, listing, inner, &look(state)),
        Content::Idle => frame.render_widget(dim("  no preview"), inner),
    }
}

fn draw_dir(frame: &mut Frame, listing: &Listing, area: Rect, look: &Look) {
    let height = area.height as usize;
    let mut lines = entry_lines(listing, height, None, area.width, look).lines;

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

fn tone_style(tone: Tone) -> Style {
    match tone {
        Tone::Plain => Style::default(),
        Tone::Comment => Style::default().add_modifier(Modifier::DIM),
        Tone::Quoted => Style::default().fg(Color::Green),
        Tone::Number => Style::default().fg(Color::Cyan),
        Tone::Constant => Style::default().fg(Color::Cyan),
        Tone::Keyword => Style::default().fg(Color::Magenta),
        Tone::Type => Style::default().fg(Color::Yellow),
        Tone::Function => Style::default().fg(Color::Blue),
    }
}

fn draw_text(frame: &mut Frame, lines: &[Vec<Piece>], truncated: bool, area: Rect) {
    if lines.is_empty() {
        frame.render_widget(dim("  (empty)"), area);
        return;
    }

    let height = area.height as usize;
    let mut shown: Vec<Line> = lines
        .iter()
        .take(height)
        .map(|pieces| {
            let spans: Vec<Span> = pieces
                .iter()
                .map(|piece| Span::styled(sanitize(&piece.text), tone_style(piece.tone)))
                .collect();
            Line::from(spans)
        })
        .collect();

    if (truncated || lines.len() > height) && shown.len() == height {
        shown.pop();
        shown.push(ellipsis());
    }

    frame.render_widget(Paragraph::new(shown), area);
}

fn heading(text: &str, area: Rect) -> Line<'static> {
    if text.is_empty() {
        return Line::default();
    }
    Line::styled(
        format!(
            " {} ",
            clip_left(text, area.width.saturating_sub(4) as usize)
        ),
        Style::default().fg(Color::Yellow),
    )
}

fn ellipsis() -> Line<'static> {
    Line::styled("…", Style::default().add_modifier(Modifier::DIM))
}

fn draw_status(frame: &mut Frame, state: &State, area: Rect) {
    if matches!(state.mode, Mode::Rename { .. }) {
        frame.render_widget(dim(" rename · enter to confirm · esc to cancel"), area);
        return;
    }

    if let Some(message) = &state.message {
        let color = if message.error {
            Color::Red
        } else {
            Color::Green
        };
        let line = Line::styled(
            format!(" {}", sanitize(&message.text)),
            Style::default().fg(color),
        );
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    let position = match state.left.cursor_row() {
        Some(row) => format!("{}/{}", row + 1, state.left.len()),
        None => "0/0".to_string(),
    };
    let hidden = if state.filter.show_hidden { " ." } else { "" };
    let marked = match state.marks.len() {
        0 => String::new(),
        n => format!(" · {n} marked"),
    };
    frame.render_widget(dim(&format!(" {position}{hidden}{marked}")), area);
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
