/// TUI toolkit — port of lib/tui.rb.
/// Screen, Line, Section, SegmentWriter, InputField.

use crate::ansi::{ansi, metrics, palette, text};
use std::io::Write;

/// Terminal size detection.
pub mod terminal {
    use std::io::IsTerminal;

    /// Get terminal size (rows, cols). Checks TRY_HEIGHT/TRY_WIDTH env first,
    /// then ioctl on stderr/stdout/stdin.
    pub fn size() -> (usize, usize) {
        let env_rows: usize = std::env::var("TRY_HEIGHT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let env_cols: usize = std::env::var("TRY_WIDTH")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let mut rows = if env_rows > 0 { Some(env_rows) } else { None };
        let mut cols = if env_cols > 0 { Some(env_cols) } else { None };

        if rows.is_none() || cols.is_none() {
            if let Some((r, c)) = ioctl_winsize(2) {
                if r > 0 { rows = rows.or(Some(r)); }
                if c > 0 { cols = cols.or(Some(c)); }
            }
        }
        if rows.is_none() || cols.is_none() {
            if std::io::stderr().is_terminal() {
                if let Some((r, c)) = ioctl_winsize(2) {
                    if r > 0 { rows = rows.or(Some(r)); }
                    if c > 0 { cols = cols.or(Some(c)); }
                }
            }
        }
        if rows.is_none() || cols.is_none() {
            if std::io::stdout().is_terminal() {
                if let Some((r, c)) = ioctl_winsize(1) {
                    if r > 0 { rows = rows.or(Some(r)); }
                    if c > 0 { cols = cols.or(Some(c)); }
                }
            }
        }
        if rows.is_none() || cols.is_none() {
            if std::io::stdin().is_terminal() {
                if let Some((r, c)) = ioctl_winsize(0) {
                    if r > 0 { rows = rows.or(Some(r)); }
                    if c > 0 { cols = cols.or(Some(c)); }
                }
            }
        }

        let rows = rows.unwrap_or(24);
        let cols = cols.unwrap_or(80);
        (rows, cols)
    }

    /// ioctl(TIOCGWINSZ) on the given file descriptor.
    #[cfg(unix)]
    fn ioctl_winsize(fd: std::os::fd::RawFd) -> Option<(usize, usize)> {
        #[repr(C)]
        struct Winsize {
            ws_row: u16,
            ws_col: u16,
            ws_xpixel: u16,
            ws_ypixel: u16,
        }

        extern "C" {
            fn ioctl(fd: std::os::fd::RawFd, request: u64, ...) -> i32;
        }

        // TIOCGWINSZ value varies by OS.
        // On Linux: 0x5413, on macOS/BSD: TIOCGWINSZ = 0x40087468 = _IOR('t', 104, struct winsize)
        #[cfg(target_os = "macos")]
        const TIOCGWINSZ: u64 = 0x40087468;
        #[cfg(target_os = "linux")]
        const TIOCGWINSZ: u64 = 0x5413;
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        const TIOCGWINSZ: u64 = 0x40087468;

        let mut ws = Winsize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let ret = unsafe { ioctl(fd, TIOCGWINSZ, &mut ws as *mut Winsize) };
        if ret == 0 && (ws.ws_row > 0 || ws.ws_col > 0) {
            Some((ws.ws_row as usize, ws.ws_col as usize))
        } else {
            None
        }
    }

    #[cfg(not(unix))]
    fn ioctl_winsize(_fd: i32) -> Option<(usize, usize)> {
        None
    }
}

/// FillSegment — a repeating pattern to fill available space.
pub struct FillSegment {
    char: String,
    style: Option<FillStyle>,
}

#[derive(Clone, Copy)]
enum FillStyle {
    Dim,
    Bold,
    Highlight,
    Accent,
}

impl FillSegment {
    fn new(ch: &str) -> Self {
        FillSegment {
            char: ch.to_string(),
            style: None,
        }
    }
    fn with_style(&self, style: FillStyle) -> Self {
        FillSegment {
            char: self.char.clone(),
            style: Some(style),
        }
    }
}

/// EmojiSegment — precomputed width.
pub struct EmojiSegment {
    char: String,
    width: usize,
    char_count: usize,
}

impl EmojiSegment {
    fn new(ch: &str) -> Self {
        let mut width = 0;
        let mut char_count = 0;
        for ch in ch.chars() {
            let w = metrics::char_width(ch as u32);
            width += w;
            if w > 0 {
                char_count += 1;
            }
        }
        EmojiSegment {
            char: ch.to_string(),
            width,
            char_count,
        }
    }

    fn width_delta(&self) -> i64 {
        self.width as i64 - self.char_count as i64
    }
}

/// A segment in a SegmentWriter — either a string, a fill, or an emoji.
enum Segment {
    Text(String),
    Fill(FillSegment),
    Emoji(EmojiSegment),
}

/// SegmentWriter — writes left/center/right content for a line.
pub struct SegmentWriter {
    segments: Vec<Segment>,
    has_wide: bool,
    width_delta: i64,
}

impl SegmentWriter {
    fn new() -> Self {
        SegmentWriter {
            segments: Vec::new(),
            has_wide: false,
            width_delta: 0,
        }
    }

    pub fn write_str(&mut self, text: &str) -> &mut Self {
        if text.is_empty() {
            return self;
        }
        self.segments.push(Segment::Text(text.to_string()));
        self
    }

    pub fn write_fill(&mut self, fill: &FillSegment) -> &mut Self {
        self.segments.push(Segment::Fill(FillSegment {
            char: fill.char.clone(),
            style: fill.style,
        }));
        self
    }

    pub fn write_emoji(&mut self, ch: &str) -> &mut Self {
        let seg = EmojiSegment::new(ch);
        self.has_wide = true;
        self.width_delta += seg.width_delta();
        self.segments.push(Segment::Emoji(seg));
        self
    }

    pub fn write_dim(&mut self, text: &str) -> &mut Self {
        // If text is a FillSegment... but we can't pattern match here.
        // The Ruby version checks if text is a FillSegment; in our API,
        // fill segments use write_fill.
        self.segments
            .push(Segment::Text(text::dim(text)));
        self
    }

    pub fn write_bold(&mut self, text: &str) -> &mut Self {
        self.segments
            .push(Segment::Text(text::bold(text)));
        self
    }

    pub fn write_highlight(&mut self, text: &str) -> &mut Self {
        self.segments
            .push(Segment::Text(text::highlight(text)));
        self
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Render to a string, given the total width context.
    pub fn to_string(&self, width: usize) -> String {
        let mut rendered = String::new();
        for seg in &self.segments {
            match seg {
                Segment::Text(s) => rendered.push_str(s),
                Segment::Emoji(e) => rendered.push_str(&e.char),
                Segment::Fill(f) => rendered.push_str(&self.render_fill(f, &rendered, width)),
            }
        }
        rendered
    }

    fn render_fill(&self, segment: &FillSegment, rendered: &str, width: usize) -> String {
        let max_fill = if width > 0 { width - 1 } else { 0 };
        let remaining = max_fill.saturating_sub(metrics::visible_width(rendered));
        if remaining == 0 {
            return String::new();
        }

        let pattern = if segment.char.is_empty() {
            " ".to_string()
        } else {
            segment.char.clone()
        };
        let pattern_width = metrics::visible_width(&pattern).max(1);
        let repeat = (remaining as f64 / pattern_width as f64).ceil() as usize;
        let mut filler = pattern.repeat(repeat);
        filler = metrics::truncate(&filler, remaining, "");
        self.apply_style(&filler, segment.style)
    }

    fn apply_style(&self, text: &str, style: Option<FillStyle>) -> String {
        match style {
            Some(FillStyle::Dim) => text::dim(text),
            Some(FillStyle::Bold) => text::bold(text),
            Some(FillStyle::Highlight) => text::highlight(text),
            Some(FillStyle::Accent) => text::accent(text),
            None => text.to_string(),
        }
    }

    /// Fast width calculation using precomputed emoji widths.
    fn visible_width(&self, rendered_str: &str) -> usize {
        let stripped = if rendered_str.contains('\x1b') {
            metrics::strip_ansi(rendered_str)
        } else {
            rendered_str.to_string()
        };
        if self.has_wide {
            stripped.chars().count() + self.width_delta as usize
        } else {
            stripped.len()
        }
    }
}

/// A line in a section — has left/center/right writers, optional background.
pub struct Line {
    background: Option<String>,
    truncate: bool,
    left: SegmentWriter,
    center: Option<SegmentWriter>,
    right: Option<SegmentWriter>,
    has_input: bool,
    input_prefix_width: usize,
}

impl Line {
    fn new(background: Option<String>, truncate: bool) -> Self {
        Line {
            background,
            truncate,
            left: SegmentWriter::new(),
            center: None,
            right: None,
            has_input: false,
            input_prefix_width: 0,
        }
    }

    pub fn left_mut(&mut self) -> &mut SegmentWriter {
        &mut self.left
    }

    pub fn center_mut(&mut self) -> &mut SegmentWriter {
        self.center.get_or_insert_with(SegmentWriter::new)
    }

    pub fn right_mut(&mut self) -> &mut SegmentWriter {
        self.right.get_or_insert_with(SegmentWriter::new)
    }

    pub fn mark_has_input(&mut self, prefix_width: usize) {
        self.has_input = true;
        self.input_prefix_width = prefix_width;
    }

    pub fn has_input(&self) -> bool {
        self.has_input
    }

    pub fn cursor_column(&self, input_cursor: usize) -> usize {
        self.input_prefix_width + input_cursor + 1
    }

    /// Render the line to a string.
    pub fn render(&self, width: usize, trailing_newline: bool) -> String {
        let mut buffer = String::new();
        buffer.push('\r');
        buffer.push_str(ansi::CLEAR_EOL);

        if let Some(bg) = &self.background {
            if crate::ansi::colors_enabled() {
                buffer.push_str(bg);
            }
        }

        let max_content = width.saturating_sub(1);
        let content_width = width.max(1);

        let mut left_text = self.left.to_string(content_width);
        let center_text = self
            .center
            .as_ref()
            .map(|c| c.to_string(content_width))
            .unwrap_or_default();
        let mut right_text = self
            .right
            .as_ref()
            .map(|r| r.to_string(content_width))
            .unwrap_or_default();

        // Truncate left to fit line
        if self.truncate && !left_text.is_empty() {
            left_text = metrics::truncate(&left_text, max_content, "…");
        }
        let left_width = if left_text.is_empty() {
            0
        } else {
            metrics::visible_width(&left_text)
        };

        // Truncate center
        let mut center_text = center_text;
        let mut center_width = 0;
        if !center_text.is_empty() {
            let max_center = max_content.saturating_sub(left_width).saturating_sub(4);
            if max_center > 0 {
                center_text = metrics::truncate(&center_text, max_center, "…");
                center_width = metrics::visible_width(&center_text);
            } else {
                center_text.clear();
            }
        }

        // Calculate available space for right
        let used_by_left_center = left_width + center_width + if center_width > 0 { 2 } else { 0 };
        let available_for_right = max_content.saturating_sub(used_by_left_center).saturating_sub(1);

        let mut right_width = 0;
        if !right_text.is_empty() {
            right_width = metrics::visible_width(&right_text);
            if available_for_right == 0 {
                right_text.clear();
                right_width = 0;
            } else if right_width > available_for_right {
                right_text = metrics::truncate_from_start(&right_text, available_for_right);
                right_width = metrics::visible_width(&right_text);
            }
        }

        // Calculate positions
        let center_col = if center_text.is_empty() {
            0
        } else {
            ((max_content.saturating_sub(center_width)) / 2).max(left_width + 1)
        };
        let right_col = if right_text.is_empty() {
            max_content
        } else {
            max_content.saturating_sub(right_width)
        };

        // Write left content
        if !left_text.is_empty() {
            buffer.push_str(&left_text);
        }
        let mut current_pos = left_width;

        // Write centered content if present
        if !center_text.is_empty() {
            let gap_to_center = center_col.saturating_sub(current_pos);
            if gap_to_center > 0 {
                buffer.push_str(&" ".repeat(gap_to_center));
            }
            buffer.push_str(&center_text);
            current_pos = center_col + center_width;
        }

        // Fill gap to right content (or end of line)
        let fill_end = if right_text.is_empty() {
            max_content
        } else {
            right_col
        };
        let gap = fill_end.saturating_sub(current_pos);
        if gap > 0 {
            buffer.push_str(&" ".repeat(gap));
        }

        // Write right content if present
        if !right_text.is_empty() {
            buffer.push_str(&right_text);
            buffer.push_str(ansi::RESET_FG);
        }

        buffer.push_str(ansi::RESET);
        if trailing_newline {
            buffer.push('\n');
        }

        buffer
    }
}

/// A section of the screen (header/body/footer).
pub struct Section {
    pub lines: Vec<Line>,
}

impl Section {
    fn new() -> Self {
        Section { lines: Vec::new() }
    }

    pub fn add_line(&mut self, background: Option<String>) -> &mut Line {
        self.lines.push(Line::new(background, true));
        self.lines.last_mut().unwrap()
    }

    pub fn add_line_no_truncate(&mut self, background: Option<String>) -> &mut Line {
        self.lines.push(Line::new(background, false));
        self.lines.last_mut().unwrap()
    }

    fn clear(&mut self) {
        self.lines.clear();
    }
}

/// The full screen — owns header, body, footer sections.
pub struct Screen {
    pub header: Section,
    pub body: Section,
    pub footer: Section,
    pub width: usize,
    pub height: usize,
}

impl Screen {
    pub fn new() -> Self {
        let (h, w) = terminal::size();
        Screen {
            header: Section::new(),
            body: Section::new(),
            footer: Section::new(),
            width: w,
            height: h,
        }
    }

    pub fn refresh_size(&mut self) {
        let (h, w) = terminal::size();
        self.height = h;
        self.width = w;
    }

    /// Render the entire frame to a single buffer string.
    pub fn flush(&mut self, input_cursor: Option<(usize, usize)>) -> String {
        self.refresh_size();
        let mut buf = String::new();
        buf.push_str(ansi::HOME);

        let mut cursor_row: Option<usize> = None;
        let mut cursor_col: Option<usize> = None;
        let mut current_row = 0usize;

        // Render header
        for line in &self.header.lines {
            if let Some((ir, ic)) = &input_cursor {
                if line.has_input() {
                    cursor_row = Some(current_row + 1);
                    cursor_col = Some(line.cursor_column(*ic));
                }
            }
            let _ = ir_ic;
            buf.push_str(&line.render(self.width, true));
            current_row += 1;
        }

        // Calculate available body space
        let footer_lines = self.footer.lines.len();
        let body_space = self.height.saturating_sub(current_row).saturating_sub(footer_lines);

        // Render body lines
        let mut body_rendered = 0;
        for line in &self.body.lines {
            if body_rendered >= body_space {
                break;
            }
            if let Some((ir, ic)) = &input_cursor {
                if line.has_input() {
                    cursor_row = Some(current_row + 1);
                    cursor_col = Some(line.cursor_column(*ic));
                }
            }
            let _ = ir_ic;
            buf.push_str(&line.render(self.width, true));
            current_row += 1;
            body_rendered += 1;
        }

        // Fill gap with blank lines
        let gap = body_space.saturating_sub(body_rendered);
        let blank_line = format!("\r{}{}", ansi::CLEAR_EOL, " ".repeat(self.width.saturating_sub(1)));
        for i in 0..gap {
            if i == gap - 1 && self.footer.lines.is_empty() {
                buf.push_str(&blank_line);
            } else {
                buf.push_str(&blank_line);
                buf.push('\n');
            }
            current_row += 1;
        }

        // Render footer
        let footer_count = self.footer.lines.len();
        for (idx, line) in self.footer.lines.iter().enumerate() {
            if let Some((ir, ic)) = &input_cursor {
                if line.has_input() {
                    cursor_row = Some(current_row + 1);
                    cursor_col = Some(line.cursor_column(*ic));
                }
            }
            let _ = ir_ic;
            let trailing = idx != footer_count - 1;
            buf.push_str(&line.render(self.width, trailing));
            current_row += 1;
        }

        // Position cursor
        if let (Some(row), Some(col), Some(_)) = (cursor_row, cursor_col, &input_cursor) {
            buf.push_str(&format!("\x1b[{};{}H", row, col));
            buf.push_str(ansi::SHOW);
        } else {
            buf.push_str(ansi::HIDE);
        }

        buf.push_str(ansi::RESET);

        // Clear sections for next frame
        self.header.clear();
        self.body.clear();
        self.footer.clear();

        buf
    }

    /// Write the frame buffer to stderr.
    pub fn render_to_stderr(&mut self, input_cursor: Option<(usize, usize)>) {
        let buf = self.flush(input_cursor);
        let _ = std::io::stderr().write_all(buf.as_bytes());
        let _ = std::io::stderr().flush();
    }
}

/// Helper functions for constructing content.
pub fn fill(char: &str) -> FillSegment {
    FillSegment::new(char)
}

pub fn emoji(char: &str) -> &str {
    // Return the emoji string; width is computed by metrics
    char
}

/// InputField — editable text field with cursor.
pub struct InputField {
    pub text: String,
    pub cursor: usize,
    pub placeholder: String,
}

impl InputField {
    pub fn new(placeholder: &str, text: &str, cursor: Option<usize>) -> Self {
        let mut field = InputField {
            text: text.to_string(),
            cursor: cursor.unwrap_or(text.chars().count()),
            placeholder: placeholder.to_string(),
        };
        field.clamp_cursor();
        field
    }

    fn clamp_cursor(&mut self) {
        let len = self.text.chars().count();
        if self.cursor > len {
            self.cursor = len;
        }
        if self.cursor == 0 && len > 0 {
            // allow 0
        }
        if self.cursor > len {
            self.cursor = len;
        }
    }

    /// Returns true if consumed as text-editing, false if the selector should handle it.
    pub fn handle_key(&mut self, key: &str) -> bool {
        if key.is_empty() {
            return false;
        }

        // Control keys
        if key == "\x7f" || key == "\x08" {
            self.backspace();
            return true;
        }
        if key == "\x1b[3~" {
            self.delete_forward();
            return true;
        }
        if key == "\x01" {
            self.cursor_home();
            return true;
        }
        if key == "\x05" {
            self.cursor_end();
            return true;
        }
        if key == "\x02" {
            self.cursor_left();
            return true;
        }
        if key == "\x06" {
            self.cursor_right();
            return true;
        }
        if key == "\x0b" {
            self.kill_to_end();
            return true;
        }
        if key == "\x15" {
            self.kill_to_start();
            return true;
        }
        if key == "\x17" {
            self.kill_word();
            return true;
        }

        // Arrow keys
        if self.left_arrow(key) {
            self.cursor_left();
            return true;
        }
        if self.right_arrow(key) {
            self.cursor_right();
            return true;
        }
        if self.home_key(key) {
            self.cursor_home();
            return true;
        }
        if self.end_key(key) {
            self.cursor_end();
            return true;
        }

        // Printable character
        if key.len() == 1 {
            let code = key.as_bytes()[0];
            if code >= 32 && code != 127 {
                self.insert(key);
                return true;
            }
            return false;
        }

        false
    }

    fn insert(&mut self, ch: &str) {
        let chars: Vec<char> = self.text.chars().collect();
        let new_char: Vec<char> = ch.chars().collect();
        if new_char.is_empty() {
            return;
        }
        let mut result: Vec<char> = Vec::with_capacity(chars.len() + 1);
        result.extend_from_slice(&chars[..self.cursor]);
        result.extend(new_char.iter().take(1));
        result.extend_from_slice(&chars[self.cursor..]);
        self.text = result.into_iter().collect();
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let chars: Vec<char> = self.text.chars().collect();
        let mut result: Vec<char> = Vec::with_capacity(chars.len() - 1);
        result.extend_from_slice(&chars[..self.cursor - 1]);
        result.extend_from_slice(&chars[self.cursor..]);
        self.text = result.into_iter().collect();
        self.cursor -= 1;
    }

    fn delete_forward(&mut self) {
        let chars: Vec<char> = self.text.chars().collect();
        if self.cursor >= chars.len() {
            return;
        }
        let mut result: Vec<char> = Vec::with_capacity(chars.len() - 1);
        result.extend_from_slice(&chars[..self.cursor]);
        result.extend_from_slice(&chars[self.cursor + 1..]);
        self.text = result.into_iter().collect();
    }

    fn kill_to_end(&mut self) {
        let chars: Vec<char> = self.text.chars().collect();
        self.text = chars[..self.cursor].iter().collect();
    }

    fn kill_to_start(&mut self) {
        let chars: Vec<char> = self.text.chars().collect();
        self.text = chars[self.cursor..].iter().collect();
        self.cursor = 0;
    }

    fn kill_word(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let chars: Vec<char> = self.text.chars().collect();
        let new_pos = self.word_boundary_backward(&chars, self.cursor);
        let mut result: Vec<char> = Vec::new();
        result.extend_from_slice(&chars[..new_pos]);
        result.extend_from_slice(&chars[self.cursor..]);
        self.text = result.into_iter().collect();
        self.cursor = new_pos;
    }

    fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    fn cursor_right(&mut self) {
        let len = self.text.chars().count();
        if self.cursor < len {
            self.cursor += 1;
        }
    }

    fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    fn cursor_end(&mut self) {
        self.cursor = self.text.chars().count();
    }

    /// Alphanumeric word boundary (Ctrl-W). Skips separators, then the word.
    fn word_boundary_backward(&self, buffer: &[char], cursor: usize) -> usize {
        let mut pos = cursor as i64 - 1;
        while pos >= 0 && !Self::alnum_char(buffer[pos as usize]) {
            pos -= 1;
        }
        while pos >= 0 && Self::alnum_char(buffer[pos as usize]) {
            pos -= 1;
        }
        (pos + 1) as usize
    }

    fn alnum_char(ch: char) -> bool {
        let c = ch as u32;
        (48..=57).contains(&c) || (65..=90).contains(&c) || (97..=122).contains(&c)
    }

    fn left_arrow(&self, key: &str) -> bool {
        if key == "\x1b[D" || key == "\x1bOD" {
            return true;
        }
        key.starts_with("\x1b[") && key.ends_with('D') && key.len() > 3
    }

    fn right_arrow(&self, key: &str) -> bool {
        if key == "\x1b[C" || key == "\x1bOC" {
            return true;
        }
        key.starts_with("\x1b[") && key.ends_with('C') && key.len() > 3
    }

    fn home_key(&self, key: &str) -> bool {
        key == "\x1b[H" || key == "\x1b[1~" || key == "\x1b[7~" || key == "\x1bOH"
    }

    fn end_key(&self, key: &str) -> bool {
        key == "\x1b[F" || key == "\x1b[4~" || key == "\x1b[8~" || key == "\x1bOF"
    }

    /// Render the input field to a string (with cursor highlight).
    pub fn render(&self) -> String {
        if self.text.is_empty() {
            return text::dim(&self.placeholder);
        }

        let chars: Vec<char> = self.text.chars().collect();
        let before: String = chars[..self.cursor].iter().collect();
        let cursor_char = if self.cursor < chars.len() {
            chars[self.cursor].to_string()
        } else {
            " ".to_string()
        };
        let after: String = if self.cursor < chars.len() {
            chars[self.cursor + 1..].iter().collect()
        } else {
            String::new()
        };

        let mut buf = String::new();
        buf.push_str(&before);
        if crate::ansi::colors_enabled() {
            buf.push_str(palette::input_cursor_on());
            buf.push_str(&cursor_char);
            buf.push_str(palette::input_cursor_off());
        } else {
            buf.push_str(&cursor_char);
        }
        buf.push_str(&after);
        buf
    }
}

// Suppress unused variable warnings for the input_cursor helper
fn ir_ic(_: &(usize, usize)) {}
