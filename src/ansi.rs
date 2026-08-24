/// ANSI escape sequences and terminal control — port of Tui::ANSI / Palette / Text.

use std::sync::atomic::{AtomicBool, Ordering};

static COLORS_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn colors_enabled() -> bool {
    COLORS_ENABLED.load(Ordering::Relaxed)
}

pub fn disable_colors() {
    COLORS_ENABLED.store(false, Ordering::Relaxed);
}

pub fn enable_colors() {
    COLORS_ENABLED.store(true, Ordering::Relaxed);
}

/// Initialize color state from env (NO_COLORS / NO_COLOR).
pub fn init_colors_from_env() {
    let no_colors = std::env::var("NO_COLORS").unwrap_or_default();
    let no_color = std::env::var("NO_COLOR").unwrap_or_default();

    if no_colors.is_empty() && no_color.is_empty() {
        enable_colors();
    } else {
        if !no_colors.is_empty() {
            disable_colors();
        }
        if !no_color.is_empty() {
            disable_colors();
        }
    }
}

pub mod ansi {
    pub const CLEAR_EOL: &str = "\x1b[K";
    pub const CLEAR_EOS: &str = "\x1b[J";
    pub const CLEAR_SCREEN: &str = "\x1b[2J";
    pub const HOME: &str = "\x1b[H";
    pub const HIDE: &str = "\x1b[?25l";
    pub const SHOW: &str = "\x1b[?25h";
    pub const CURSOR_BLINK: &str = "\x1b[1 q";
    pub const CURSOR_STEADY: &str = "\x1b[2 q";
    pub const CURSOR_DEFAULT: &str = "\x1b[0 q";
    pub const ALT_SCREEN_ON: &str = "\x1b[?1049h";
    pub const ALT_SCREEN_OFF: &str = "\x1b[?1049l";
    pub const RESET: &str = "\x1b[0m";
    pub const RESET_FG: &str = "\x1b[39m";
    pub const RESET_BG: &str = "\x1b[49m";
    pub const RESET_INTENSITY: &str = "\x1b[22m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";

    pub fn fg(code: u32) -> String {
        format!("\x1b[38;5;{}m", code)
    }

    pub fn bg(code: u32) -> String {
        format!("\x1b[48;5;{}m", code)
    }

    pub fn move_col(col: usize) -> String {
        format!("\x1b[{}G", col)
    }

    pub fn sgr(codes: &[&str]) -> String {
        let joined = codes.join(";");
        format!("\x1b[{}m", joined)
    }

    pub fn set_title(t: &str) -> String {
        format!("\x1b]2;{}\x07", t)
    }
}

pub mod palette {
    use super::ansi;

    pub fn header() -> String {
        ansi::sgr(&["1", "38;5;114"])
    }
    pub fn accent() -> String {
        ansi::sgr(&["1", "38;5;214"])
    }
    pub fn highlight() -> &'static str {
        "\x1b[1;33m"
    }
    pub fn muted() -> String {
        ansi::fg(245)
    }
    pub fn match_color() -> String {
        ansi::sgr(&["1", "38;5;226"])
    }
    pub fn input_hint() -> String {
        ansi::fg(244)
    }
    pub fn input_cursor_on() -> &'static str {
        "\x1b[7m"
    }
    pub fn input_cursor_off() -> &'static str {
        "\x1b[27m"
    }
    pub fn selected_bg() -> String {
        ansi::bg(238)
    }
    pub fn selected_fg() -> String {
        ansi::fg(255)
    }
    pub fn danger_bg() -> String {
        ansi::bg(52)
    }
}

pub mod text {
    use super::{ansi, colors_enabled};

    pub fn bold(text: &str) -> String {
        wrap(text, ansi::BOLD, ansi::RESET_INTENSITY)
    }

    pub fn dim(text: &str) -> String {
        wrap(text, &super::palette::muted(), ansi::RESET_FG)
    }

    pub fn highlight(text: &str) -> String {
        wrap(
            text,
            super::palette::highlight(),
            &format!("{}{}", ansi::RESET_FG, ansi::RESET_INTENSITY),
        )
    }

    pub fn accent(text: &str) -> String {
        wrap(
            text,
            &super::palette::accent(),
            &format!("{}{}", ansi::RESET_FG, ansi::RESET_INTENSITY),
        )
    }

    fn wrap(text: &str, prefix: &str, suffix: &str) -> String {
        if text.is_empty() {
            return String::new();
        }
        if !colors_enabled() {
            return text.to_string();
        }
        format!("{}{}{}", prefix, text, suffix)
    }
}

/// Unicode width metrics — port of Tui::Metrics.
pub mod metrics {
    const ANSI_STRIP_RE_END: [u8; 1] = [0];

    /// Strip ANSI escape sequences from text.
    pub fn strip_ansi(text: &str) -> String {
        let mut result = String::new();
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == 0x1b {
                // Skip escape sequence
                i += 1;
                while i < bytes.len() {
                    let c = bytes[i];
                    i += 1;
                    if (c as char).is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                // Collect a UTF-8 char
                let start = i;
                let len = utf8_char_len(bytes[i]);
                if i + len <= bytes.len() {
                    result.push_str(&text[start..start + len]);
                    i += len;
                } else {
                    break;
                }
            }
        }
        let _ = ANSI_STRIP_RE_END;
        result
    }

    /// Calculate visible width of text, accounting for ANSI escapes and wide chars.
    pub fn visible_width(text: &str) -> usize {
        let has_escape = text.contains('\x1b');

        // Fast path: pure ASCII with no escapes
        if !has_escape && text.is_ascii() {
            return text.len();
        }

        let stripped = if has_escape { strip_ansi(text) } else { text.to_string() };

        // Fast path after stripping: pure ASCII
        if stripped.is_ascii() {
            return stripped.len();
        }

        // Slow path: calculate width per codepoint
        let mut width = 0;
        for ch in stripped.chars() {
            width += char_width(ch as u32);
        }
        width
    }

    /// Simplified width check — matches the Ruby char_width logic.
    pub fn char_width(code: u32) -> usize {
        // Zero-width: variation selectors (🗑️ = trash + VS16)
        if (0xFE00..=0xFE0F).contains(&code) {
            return 0;
        }
        // Emoji range (📁🏠🗑📂 etc) = width 2
        if (0x1F300..=0x1FAFF).contains(&code) {
            return 2;
        }
        // Everything else (ASCII, arrows, box drawing, ellipsis) = width 1
        1
    }

    pub fn zero_width(ch: char) -> bool {
        let code = ch as u32;
        (0xFE00..=0xFE0F).contains(&code)
            || (0x200B..=0x200D).contains(&code)
            || (0x0300..=0x036F).contains(&code)
            || (0xE0100..=0xE01EF).contains(&code)
    }

    pub fn wide(ch: char) -> bool {
        char_width(ch as u32) == 2
    }

    /// Truncate text to max_width, appending overflow.
    pub fn truncate(text: &str, max_width: usize, overflow: &str) -> String {
        if visible_width(text) <= max_width {
            return text.to_string();
        }

        let overflow_width = visible_width(overflow);
        let target = max_width.saturating_sub(overflow_width);

        let mut truncated = String::new();
        let mut width = 0;
        let mut in_escape = false;
        let mut escape_buf = String::new();

        for ch in text.chars() {
            if in_escape {
                escape_buf.push(ch);
                if ch.is_ascii_alphabetic() {
                    truncated.push_str(&escape_buf);
                    escape_buf.clear();
                    in_escape = false;
                }
                continue;
            }

            if ch == '\x1b' {
                in_escape = true;
                escape_buf.clear();
                escape_buf.push(ch);
                continue;
            }

            let cw = char_width(ch as u32);
            if width + cw > target {
                break;
            }
            truncated.push(ch);
            width += cw;
        }

        format!("{}{}", truncated.trim_end(), overflow)
    }

    /// Truncate from the start, keeping trailing portion.
    /// Preserves leading ANSI escape sequences.
    pub fn truncate_from_start(text: &str, max_width: usize) -> String {
        let vis_width = visible_width(text);
        if vis_width <= max_width {
            return text.to_string();
        }

        // Collect leading escape sequences first
        let mut leading_escapes = String::new();
        let mut in_escape = false;
        let mut escape_buf = String::new();

        for ch in text.chars() {
            if in_escape {
                escape_buf.push(ch);
                if ch.is_ascii_alphabetic() {
                    leading_escapes.push_str(&escape_buf);
                    escape_buf.clear();
                    in_escape = false;
                }
            } else if ch == '\x1b' {
                in_escape = true;
                escape_buf.clear();
                escape_buf.push(ch);
            } else {
                break;
            }
        }

        // Skip visible characters to get max_width remaining
        let chars_to_skip = vis_width - max_width;
        let mut skipped = 0;
        let mut result = String::new();
        let mut in_escape = false;

        for ch in text.chars() {
            if in_escape {
                if skipped >= chars_to_skip {
                    result.push(ch);
                }
                if ch.is_ascii_alphabetic() {
                    in_escape = false;
                }
                continue;
            }

            if ch == '\x1b' {
                in_escape = true;
                if skipped >= chars_to_skip {
                    result.push(ch);
                }
                continue;
            }

            let cw = char_width(ch as u32);
            if skipped < chars_to_skip {
                skipped += cw;
            } else {
                result.push(ch);
            }
        }

        format!("{}{}", leading_escapes, result)
    }

    /// Get the byte length of a UTF-8 character from its first byte.
    fn utf8_char_len(first_byte: u8) -> usize {
        if first_byte < 0x80 {
            1
        } else if first_byte < 0xC0 {
            1 // continuation byte
        } else if first_byte < 0xE0 {
            2
        } else if first_byte < 0xF0 {
            3
        } else {
            4
        }
    }
}
