use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthStr;

pub enum Action {
    Continue,
    Commit,
    Cancel,
}

pub struct Edit {
    text: String,
    cursor: usize,
}

impl Edit {
    pub fn new(text: String) -> Edit {
        let cursor = match text.rfind('.') {
            Some(dot) if dot > 0 => dot,
            _ => text.len(),
        };
        Edit { text, cursor }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn caret(&self) -> usize {
        self.text[..self.cursor].width()
    }

    pub fn apply(&mut self, key: KeyEvent) -> Action {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let word = ctrl || alt;

        match key.code {
            KeyCode::Esc => return Action::Cancel,
            KeyCode::Enter => return Action::Commit,
            KeyCode::Left if word => self.word_left(),
            KeyCode::Right if word => self.word_right(),
            KeyCode::Left => self.left(),
            KeyCode::Right => self.right(),
            KeyCode::Home => self.home(),
            KeyCode::End => self.end(),
            KeyCode::Backspace if word => self.delete_word_back(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete if word => self.delete_word_forward(),
            KeyCode::Delete => self.delete(),
            KeyCode::Char('a') if ctrl => self.home(),
            KeyCode::Char('e') if ctrl => self.end(),
            KeyCode::Char('b') if ctrl => self.left(),
            KeyCode::Char('f') if ctrl => self.right(),
            KeyCode::Char('u') if ctrl => self.kill_to_start(),
            KeyCode::Char('k') if ctrl => self.kill_to_end(),
            KeyCode::Char('w') if ctrl => self.delete_word_back(),
            KeyCode::Char('h') if ctrl => self.delete_word_back(),
            KeyCode::Char('b') if alt => self.word_left(),
            KeyCode::Char('f') if alt => self.word_right(),
            KeyCode::Char('d') if alt => self.delete_word_forward(),
            KeyCode::Char(c) if !ctrl && !alt => self.insert(c),
            _ => {}
        }
        Action::Continue
    }

    fn insert(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    fn backspace(&mut self) {
        let start = self.prev();
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    fn delete(&mut self) {
        let end = self.next();
        self.text.replace_range(self.cursor..end, "");
    }

    fn delete_word_back(&mut self) {
        let start = self.word_start();
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    fn delete_word_forward(&mut self) {
        let end = self.word_end();
        self.text.replace_range(self.cursor..end, "");
    }

    fn kill_to_start(&mut self) {
        self.text.replace_range(..self.cursor, "");
        self.cursor = 0;
    }

    fn kill_to_end(&mut self) {
        self.text.truncate(self.cursor);
    }

    fn left(&mut self) {
        self.cursor = self.prev();
    }

    fn right(&mut self) {
        self.cursor = self.next();
    }

    fn word_left(&mut self) {
        self.cursor = self.word_start();
    }

    fn word_right(&mut self) {
        self.cursor = self.word_end();
    }

    fn home(&mut self) {
        self.cursor = 0;
    }

    fn end(&mut self) {
        self.cursor = self.text.len();
    }

    fn prev(&self) -> usize {
        match self.text[..self.cursor].chars().next_back() {
            Some(c) => self.cursor - c.len_utf8(),
            None => 0,
        }
    }

    fn next(&self) -> usize {
        match self.text[self.cursor..].chars().next() {
            Some(c) => self.cursor + c.len_utf8(),
            None => self.cursor,
        }
    }

    fn word_start(&self) -> usize {
        let mut i = self.cursor;
        while let Some(c) = self.text[..i].chars().next_back() {
            if c.is_alphanumeric() {
                break;
            }
            i -= c.len_utf8();
        }
        while let Some(c) = self.text[..i].chars().next_back() {
            if !c.is_alphanumeric() {
                break;
            }
            i -= c.len_utf8();
        }
        i
    }

    fn word_end(&self) -> usize {
        let mut i = self.cursor;
        while let Some(c) = self.text[i..].chars().next() {
            if c.is_alphanumeric() {
                break;
            }
            i += c.len_utf8();
        }
        while let Some(c) = self.text[i..].chars().next() {
            if !c.is_alphanumeric() {
                break;
            }
            i += c.len_utf8();
        }
        i
    }
}
