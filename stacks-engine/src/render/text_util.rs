//! Unicode-aware text utilities for TUI rendering.
//!
//! All width calculations use display columns (terminal cells), not byte length.

use unicode_width::UnicodeWidthStr;
use unicode_width::UnicodeWidthChar;

/// Display width in terminal columns.
pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Truncate string to fit within `max_cols` display columns.
/// Appends "..." if truncated.
pub fn truncate_to_cols(s: &str, max_cols: usize) -> String {
    let w = display_width(s);
    if w <= max_cols {
        return s.to_string();
    }
    // Reserve 1 column for ellipsis
    let target = max_cols.saturating_sub(1);
    let mut width = 0;
    let truncated: String = s.chars().take_while(|c| {
        let cw = UnicodeWidthChar::width(*c).unwrap_or(0);
        if width + cw > target {
            return false;
        }
        width += cw;
        true
    }).collect();
    format!("{}\u{2026}", truncated)
}

/// Word-wrap text to `width` display columns. Preserves newlines.
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    for source_line in text.split('\n') {
        if display_width(source_line) <= width {
            lines.push(source_line.to_string());
            continue;
        }
        let words: Vec<&str> = source_line.split_whitespace().collect();
        if words.is_empty() {
            lines.push(String::new());
            continue;
        }
        // Preserve leading whitespace
        let trimmed = source_line.trim_start();
        let leading = &source_line[..source_line.len() - trimmed.len()];
        let mut current = String::from(leading);
        for word in words {
            if current.trim().is_empty() {
                current = format!("{}{}", leading, word);
            } else if display_width(&current) + 1 + display_width(word) <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(current);
                current = format!("{}{}", leading, word);
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Word-wrap without indent preservation. Returns vec of line chunks.
pub fn wrap_to_cols(text: &str, width: usize) -> Vec<String> {
    if width == 0 || display_width(text) <= width {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![String::new()];
    }
    let mut current = String::new();
    for word in words {
        if current.is_empty() {
            current = word.to_string();
        } else if display_width(&current) + 1 + display_width(word) <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Center string within `width` display columns. Truncates if wider.
pub fn pad_center(s: &str, width: usize) -> String {
    let w = display_width(s);
    if w >= width {
        // Truncate to width
        let mut col = 0;
        let truncated: String = s.chars().take_while(|c| {
            let cw = UnicodeWidthChar::width(*c).unwrap_or(0);
            if col + cw > width {
                return false;
            }
            col += cw;
            true
        }).collect();
        return truncated;
    }
    let total_pad = width - w;
    let left = total_pad / 2;
    let right = total_pad - left;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
}

/// Safe string truncation at a byte position, snapping to char boundary.
/// Returns `&s[..pos]` where pos is adjusted to the nearest valid char boundary.
pub fn safe_prefix(s: &str, max_bytes: usize) -> &str {
    let end = s.floor_char_boundary(max_bytes.min(s.len()));
    &s[..end]
}
