use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::listing::Listing;
use crate::preview::Preview;
use crate::state::{Content, State};

const HIDDEN: Color = Color::Indexed(245);

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

    let lines = entry_lines(&state.left, inner.height as usize, state.left.cursor_row());

    if lines.is_empty() {
        frame.render_widget(dim("  (empty)"), inner);
    } else {
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

fn entry_lines(listing: &Listing, height: usize, cursor: Option<usize>) -> Vec<Line<'static>> {
    rows(listing, height)
        .map(|(row, entry)| {
            let mut style = Style::default();
            if entry.is_dir() {
                style = style.fg(Color::Blue).add_modifier(Modifier::BOLD);
            }
            if entry.is_hidden() {
                style = style.fg(HIDDEN);
            }
            if Some(row) == cursor {
                style = style.bg(Color::DarkGray);
            }
            let suffix = if entry.is_dir() { "/" } else { "" };
            Line::styled(format!(" {}{suffix}", sanitize(&entry.display())), style)
        })
        .collect()
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
        Content::Dir(listing) => draw_dir(frame, listing, inner),
        Content::Idle => frame.render_widget(dim("  no preview"), inner),
    }
}

fn draw_dir(frame: &mut Frame, listing: &Listing, area: Rect) {
    let height = area.height as usize;
    let mut lines = entry_lines(listing, height, None);

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
    Line::styled("…", Style::default().fg(Color::DarkGray))
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

fn rows(
    listing: &Listing,
    height: usize,
) -> impl Iterator<Item = (usize, &crate::listing::Entry)> {
    listing
        .entries()
        .enumerate()
        .skip(listing.top)
        .take(height)
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .collect()
}

fn dim(s: &str) -> Paragraph<'static> {
    Paragraph::new(Line::styled(
        s.to_string(),
        Style::default().fg(Color::DarkGray),
    ))
}
