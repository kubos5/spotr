//! A single-line text field with readline-style editing.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Default, Clone, Debug)]
pub struct TextInput {
    pub value: String,
    /// Cursor position, in chars.
    pub cursor: usize,
}

impl TextInput {
    pub fn with(s: &str) -> Self {
        Self { value: s.to_string(), cursor: s.chars().count() }
    }

    fn byte(&self, char_idx: usize) -> usize {
        self.value.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(self.value.len())
    }

    pub fn len(&self) -> usize {
        self.value.chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    pub fn set(&mut self, s: &str) {
        self.value = s.to_string();
        self.cursor = self.len();
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    pub fn insert_str(&mut self, s: &str) {
        let s: String = s.chars().filter(|c| !c.is_control()).collect();
        let b = self.byte(self.cursor);
        self.value.insert_str(b, &s);
        self.cursor += s.chars().count();
    }

    pub fn insert(&mut self, c: char) {
        let b = self.byte(self.cursor);
        self.value.insert(b, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        let b = self.byte(self.cursor);
        self.value.remove(b);
        true
    }

    pub fn delete(&mut self) -> bool {
        if self.cursor >= self.len() {
            return false;
        }
        let b = self.byte(self.cursor);
        self.value.remove(b);
        true
    }

    fn word_left(&self) -> usize {
        let chars: Vec<char> = self.value.chars().collect();
        let mut i = self.cursor;
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !chars[i - 1].is_whitespace() {
            i -= 1;
        }
        i
    }

    fn word_right(&self) -> usize {
        let chars: Vec<char> = self.value.chars().collect();
        let mut i = self.cursor;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        i
    }

    pub fn delete_word(&mut self) -> bool {
        let start = self.word_left();
        if start == self.cursor {
            return false;
        }
        let (a, b) = (self.byte(start), self.byte(self.cursor));
        self.value.replace_range(a..b, "");
        self.cursor = start;
        true
    }

    /// Handle an editing key. Returns `Some(changed)` if the key was consumed.
    pub fn handle(&mut self, key: &KeyEvent) -> Option<bool> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char('w') if ctrl => Some(self.delete_word()),
            KeyCode::Backspace if ctrl || alt => Some(self.delete_word()),
            KeyCode::Char('u') if ctrl => {
                let b = self.byte(self.cursor);
                let changed = b > 0;
                self.value.replace_range(..b, "");
                self.cursor = 0;
                Some(changed)
            }
            KeyCode::Char('k') if ctrl => {
                let b = self.byte(self.cursor);
                let changed = b < self.value.len();
                self.value.truncate(b);
                Some(changed)
            }
            KeyCode::Char('a') if ctrl => {
                self.cursor = 0;
                Some(false)
            }
            KeyCode::Char('e') if ctrl => {
                self.cursor = self.len();
                Some(false)
            }
            KeyCode::Char('b') if alt => {
                self.cursor = self.word_left();
                Some(false)
            }
            KeyCode::Char('f') if alt => {
                self.cursor = self.word_right();
                Some(false)
            }
            KeyCode::Char(c) if !ctrl && !alt => {
                self.insert(c);
                Some(true)
            }
            KeyCode::Backspace => Some(self.backspace()),
            KeyCode::Delete => Some(self.delete()),
            KeyCode::Left if ctrl || alt => {
                self.cursor = self.word_left();
                Some(false)
            }
            KeyCode::Right if ctrl || alt => {
                self.cursor = self.word_right();
                Some(false)
            }
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                Some(false)
            }
            KeyCode::Right => {
                self.cursor = (self.cursor + 1).min(self.len());
                Some(false)
            }
            KeyCode::Home => {
                self.cursor = 0;
                Some(false)
            }
            KeyCode::End => {
                self.cursor = self.len();
                Some(false)
            }
            _ => None,
        }
    }

    /// Text visible in a field of `width` cells, plus the cursor column.
    pub fn view(&self, width: usize) -> (String, usize) {
        use unicode_width::UnicodeWidthChar;
        let chars: Vec<char> = self.value.chars().collect();
        let mut start = 0;
        let width = width.max(1);
        loop {
            let w: usize = chars[start..self.cursor].iter().map(|c| c.width().unwrap_or(0)).sum();
            if w < width || start >= self.cursor {
                break;
            }
            start += 1;
        }
        let mut out = String::new();
        let mut w = 0;
        for c in &chars[start..] {
            let cw = c.width().unwrap_or(0);
            if w + cw > width {
                break;
            }
            out.push(*c);
            w += cw;
        }
        let cursor_col: usize = chars[start..self.cursor].iter().map(|c| c.width().unwrap_or(0)).sum();
        (out, cursor_col)
    }
}
