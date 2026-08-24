/// TrySelector — the interactive directory selector.
/// Port of the TrySelector class in try.rb.

use crate::ansi::{ansi, metrics, palette, text};
use crate::tui;
use crate::fuzzy::{self, DirEntry};
use crate::term;
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct TrySelector {
    search_term: String,
    cursor_pos: usize,
    scroll_offset: usize,
    search: tui::InputField,
    selected: Option<Selection>,
    base_path: String,
    all_tries: Option<Vec<DirEntry>>,
    delete_status: Option<String>,
    delete_mode: bool,
    marked_for_deletion: Vec<String>,
    test_render_once: bool,
    test_no_cls: bool,
    test_keys: Vec<String>,
    test_had_keys: bool,
    test_confirm: Option<String>,
    needs_redraw: bool,
    // Cached fuzzy results
    cached_query: String,
    cached_results: Vec<fuzzy::MatchResult>,
}

#[derive(Clone)]
pub enum Selection {
    Cd { path: String },
    Mkdir { path: String },
    Delete { paths: Vec<(String, String)>, base_path: String },
    Rename { old: String, new: String, base_path: String },
    Ascend { source: String, dest: String, basename: String, base_path: String },
    Cancel,
}

pub fn default_try_path() -> String {
    std::env::var("TRY_PATH").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        format!("{}/src/tries", home)
    })
}

pub fn try_projects() -> Option<String> {
    std::env::var("TRY_PROJECTS").ok().filter(|s| !s.is_empty())
}

impl TrySelector {
    pub fn new(
        search_term: &str,
        base_path: &str,
        initial_input: Option<&str>,
        test_render_once: bool,
        test_no_cls: bool,
        test_keys: Vec<String>,
        test_confirm: Option<String>,
    ) -> Self {
        let search_term = search_term.replace(char::is_whitespace, "-");
        let initial = initial_input
            .map(|s| s.replace(char::is_whitespace, "-"))
            .unwrap_or_else(|| search_term.clone());
        let test_had_keys = !test_keys.is_empty();

        TrySelector {
            search_term,
            cursor_pos: 0,
            scroll_offset: 0,
            search: tui::InputField::new("", &initial, None),
            selected: None,
            base_path: base_path.to_string(),
            all_tries: None,
            delete_status: None,
            delete_mode: false,
            marked_for_deletion: Vec::new(),
            test_render_once,
            test_no_cls,
            test_keys,
            test_had_keys,
            test_confirm,
            needs_redraw: false,
            cached_query: String::new(),
            cached_results: Vec::new(),
        }
    }

    pub fn run(&mut self) -> Option<Selection> {
        self.setup_terminal();

        if self.test_render_once && self.test_keys.is_empty() {
            let tries = self.get_tries();
            self.render(&tries);
            return None;
        }

        if !term::is_stdin_tty() || !term::is_stderr_tty() {
            if self.test_keys.is_empty() {
                let _ = writeln!(std::io::stderr(), "Error: try requires an interactive terminal");
                return None;
            }
            self.main_loop();
        } else {
            let saved = term::enable_raw_mode();
            self.main_loop();
            if let Some(state) = saved {
                term::stty_set(&state);
            }
        }
        self.restore_terminal();
        self.selected.clone()
    }

    fn setup_terminal(&mut self) {
        if !self.test_no_cls {
            let _ = write!(
                std::io::stderr(),
                "{}{}{}",
                ansi::ALT_SCREEN_ON,
                ansi::set_title("try"),
                ansi::CURSOR_BLINK
            );
            let _ = std::io::stderr().flush();
        }
    }

    fn restore_terminal(&mut self) {
        if self.test_no_cls {
            return;
        }
        let _ = write!(
            std::io::stderr(),
            "{}{}{}",
            ansi::RESET,
            ansi::CURSOR_DEFAULT,
            ansi::ALT_SCREEN_OFF
        );
        let _ = std::io::stderr().flush();
        term::stdin_iflush();
    }

    fn load_all_tries(&mut self) -> Vec<DirEntry> {
        if let Some(ref tries) = self.all_tries {
            return tries.clone();
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        let mut tries = Vec::new();
        let base = Path::new(&self.base_path);

        if let Ok(entries) = std::fs::read_dir(&self.base_path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }

                let path = base.join(&name);
                let path_str = path.to_string_lossy().to_string();

                let lmeta = match std::fs::symlink_metadata(&path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let is_symlink = lmeta.file_type().is_symlink();

                // Use metadata() (follows symlinks) for is_dir() so symlinked
                // directories are included, matching Ruby's File.stat(path).directory?
                let metadata = match std::fs::metadata(&path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                if !metadata.is_dir() {
                    continue;
                }

                let mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                let ctime = metadata
                    .created()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(mtime);

                let hours_since_access = (now - mtime) / 3600.0;
                let base_score = 3.0 / (hours_since_access + 1.0).sqrt();

                let is_date_prefixed = name.len() >= 11
                    && name.as_bytes()[4] == b'-'
                    && name.as_bytes()[7] == b'-'
                    && name.as_bytes()[10] == b'-'
                    && name.as_bytes()[0..4].iter().all(|b| b.is_ascii_digit())
                    && name.as_bytes()[5..7].iter().all(|b| b.is_ascii_digit())
                    && name.as_bytes()[8..10].iter().all(|b| b.is_ascii_digit());

                let base_score = if is_date_prefixed {
                    base_score + 2.0
                } else {
                    base_score
                };

                let real_path = if is_symlink {
                    std::fs::canonicalize(&path)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or(path_str.clone())
                } else {
                    path_str.clone()
                };

                tries.push(DirEntry {
                    text: name.clone(),
                    text_lower: name.to_lowercase(),
                    base_score,
                    path: real_path,
                    is_symlink,
                    ctime_secs: ctime,
                    mtime_secs: mtime,
                });
            }
        }

        self.all_tries = Some(tries.clone());
        tries
    }

    fn get_tries(&mut self) -> Vec<fuzzy::MatchResult> {
        let all = self.load_all_tries();

        // Cache results — only re-match when query changes
        if self.cached_query == self.search.text && !self.cached_results.is_empty() {
            return self.cached_results.clone();
        }
        // Also return on empty query (match all)
        if self.cached_query == self.search.text && !self.cached_results.is_empty() {
            return self.cached_results.clone();
        }

        self.cached_query = self.search.text.clone();
        let (height, _) = tui::terminal::size();
        let max_results = height.saturating_sub(6).max(3);

        let mut results = fuzzy::fuzzy_match(&all, &self.search.text);
        if results.len() > max_results {
            results.truncate(max_results);
        }
        self.cached_results = results.clone();
        results
    }

    fn main_loop(&mut self) {
        loop {
            let tries = self.get_tries();
            let show_create_new = !self.search.text.is_empty();
            let total_items = tries.len() + if show_create_new { 1 } else { 0 };

            self.cursor_pos = self.cursor_pos.min(total_items.saturating_sub(1));

            self.render(&tries);

            let key = match self.read_key() {
                Some(k) => k,
                None => continue,
            };

            let before = self.search.text.clone();
            if self.search.handle_key(&key) {
                if self.search.text != before {
                    self.cursor_pos = 0;
                }
                continue;
            }

            match key.as_str() {
                "\r" => {
                    // Enter
                    if self.delete_mode && !self.marked_for_deletion.is_empty() {
                        self.confirm_batch_delete(&tries);
                        if self.selected.is_some() {
                            break;
                        }
                    } else if self.cursor_pos < tries.len() {
                        self.handle_selection(&tries[self.cursor_pos]);
                        if self.selected.is_some() {
                            break;
                        }
                    } else if show_create_new {
                        self.handle_create_new();
                        if self.selected.is_some() {
                            break;
                        }
                    }
                }
                "\x1b[A" | "\x10" | "\x0b" => {
                    // Up arrow, Ctrl-P, or Ctrl-K
                    self.cursor_pos = self.cursor_pos.saturating_sub(1);
                }
                "\x1b[B" | "\x0e" | "\x0a" => {
                    // Down arrow, Ctrl-N, or Ctrl-J
                    self.cursor_pos = (self.cursor_pos + 1).min(total_items.saturating_sub(1));
                }
                "\x04" => {
                    // Ctrl-D - toggle mark for deletion
                    if self.cursor_pos < tries.len() {
                        let path = tries[self.cursor_pos].entry().path.clone();
                        if let Some(pos) = self.marked_for_deletion.iter().position(|p| *p == path) {
                            self.marked_for_deletion.remove(pos);
                        } else {
                            self.marked_for_deletion.push(path);
                            self.delete_mode = true;
                        }
                        if self.marked_for_deletion.is_empty() {
                            self.delete_mode = false;
                        }
                    }
                }
                "\x14" => {
                    // Ctrl-T - create new try (immediate)
                    self.handle_create_new();
                    if self.selected.is_some() {
                        break;
                    }
                }
                "\x12" => {
                    // Ctrl-R - rename selected entry
                    if self.cursor_pos < tries.len() {
                        let entry = tries[self.cursor_pos].entry().clone();
                        self.run_rename_dialog(&entry);
                        if self.selected.is_some() {
                            break;
                        }
                    }
                }
                "\x07" => {
                    // Ctrl-G - graduate/ascend selected entry
                    if self.cursor_pos < tries.len() {
                        let entry = tries[self.cursor_pos].entry().clone();
                        self.run_ascend_dialog(&entry);
                        if self.selected.is_some() {
                            break;
                        }
                    }
                }
                "\x03" | "\x1b" => {
                    // Ctrl-C or ESC
                    if self.delete_mode {
                        self.marked_for_deletion.clear();
                        self.delete_mode = false;
                    } else {
                        self.selected = Some(Selection::Cancel);
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    fn read_key(&mut self) -> Option<String> {
        if !self.test_keys.is_empty() {
            return Some(self.test_keys.remove(0));
        }
        if self.test_had_keys && self.test_keys.is_empty() {
            return Some("\x1b".to_string());
        }

        loop {
            if self.needs_redraw {
                self.needs_redraw = false;
                self.clear_screen();
                return None;
            }
            // Poll stdin with timeout
            if let Some(true) = self.poll_stdin(100) { return term::read_keypress() }
        }
    }

    #[cfg(unix)]
    fn poll_stdin(&self, timeout_ms: u64) -> Option<bool> {
        use std::os::fd::AsRawFd;

        let fd = std::io::stdin().as_raw_fd();
        let mut fds = [FdSet::default()];
        fds[0].fd = fd;
        fds[0].events = POLLIN;

        extern "C" {
            fn poll(fds: *mut FdSet, nfds: u64, timeout: i32) -> i32;
        }

        // Use poll with millisecond timeout
        let ret = unsafe { poll(fds.as_mut_ptr(), 1, timeout_ms as i32) };
        if ret > 0 && (fds[0].revents & POLLIN) != 0 {
            Some(true)
        } else {
            None
        }
    }

    #[cfg(not(unix))]
    fn poll_stdin(&self, _timeout_ms: u64) -> Option<bool> {
        None
    }

    fn clear_screen(&self) {
        let _ = write!(std::io::stderr(), "\x1b[2J\x1b[H");
        let _ = std::io::stderr().flush();
    }

    fn render(&mut self, tries: &[fuzzy::MatchResult]) {
        let mut screen = tui::Screen::new();
        let width = screen.width;

        // Header
        {
            let line = screen.header.add_line(None);
            line.left_mut().write_str(&emoji("🏠"));
            line.left_mut().write_str(&text::accent(" Try Directory Selection"));

            let line = screen.header.add_line(None);
            line.left_mut().write_fill(&tui::fill("─"));

            let line = screen.header.add_line(None);
            let prefix = "Search: ";
            line.left_mut().write_dim(prefix);
            line.left_mut().write_str(&self.search.render());
            line.mark_has_input(metrics::visible_width(prefix));

            let line = screen.header.add_line(None);
            line.left_mut().write_fill(&tui::fill("─"));
        }

        // Footer
        {
            let line = screen.footer.add_line(None);
            line.left_mut().write_fill(&tui::fill("─"));

            if let Some(ref status) = self.delete_status {
                let line = screen.footer.add_line(None);
                line.left_mut().write_bold(status);
                self.delete_status = None;
            } else if self.delete_mode {
                let line = screen.footer.add_line(Some(palette::danger_bg()));
                line.left_mut().write_bold(" DELETE MODE ");
                line.left_mut().write_str(&format!(
                    " {} marked  |  Ctrl-D: Toggle  Enter: Confirm  Esc: Cancel",
                    self.marked_for_deletion.len()
                ));
            } else {
                let line = screen.footer.add_line(None);
                line.center_mut().write_dim("↑/↓: Navigate  Enter: Select  ^R: Rename  ^G: Graduate  ^D: Delete  Esc: Cancel");
            }
        }

        // Body
        let header_lines = screen.header.lines.len();
        let footer_lines = screen.footer.lines.len();
        let max_visible = screen.height.saturating_sub(header_lines).saturating_sub(footer_lines).max(3);
        let show_create_new = !self.search.text.is_empty();
        let total_items = tries.len() + if show_create_new { 1 } else { 0 };

        // Scroll
        if self.cursor_pos < self.scroll_offset {
            self.scroll_offset = self.cursor_pos;
        } else if self.cursor_pos >= self.scroll_offset + max_visible {
            self.scroll_offset = self.cursor_pos.saturating_sub(max_visible) + 1;
        }

        let visible_end = (self.scroll_offset + max_visible).min(total_items);

        for idx in self.scroll_offset..visible_end {
            if idx == tries.len() && !tries.is_empty() && idx >= self.scroll_offset {
                screen.body.add_line(None);
            }

            if idx < tries.len() {
                self.render_entry_line(&mut screen, &tries[idx], idx == self.cursor_pos, width);
            } else {
                self.render_create_line(&mut screen, idx == self.cursor_pos, width);
            }
        }

        screen.render_to_stderr(Some((self.search.cursor, 0)));
    }

    fn render_entry_line(
        &self,
        screen: &mut tui::Screen,
        entry: &fuzzy::MatchResult,
        is_selected: bool,
        width: usize,
    ) {
        let path = entry.entry().path.clone();
        let is_marked = self.marked_for_deletion.contains(&path);

        let background = if is_marked {
            let mut bg = palette::danger_bg();
            if is_selected {
                bg.push_str(&palette::selected_fg());
            }
            Some(bg)
        } else if is_selected {
            Some(format!("{}{}", palette::selected_bg(), palette::selected_fg()))
        } else {
            None
        };

        let line = screen.body.add_line(background);

        // Arrow + spacing
        if is_selected {
            line.left_mut().write_str(&text::highlight("→ "));
            line.left_mut().write_str(&self.selected_foreground());
        } else {
            line.left_mut().write_str("  ");
        }

        let icon = if is_marked {
            "🗑️"
        } else if entry.entry().is_symlink {
            "🔗"
        } else {
            "📁"
        };
        line.left_mut().write_str(&emoji(icon));
        line.left_mut().write_str(" ");

        let (plain_name, rendered_name) = self.formatted_entry_name(entry, is_selected);
        let prefix_width = 5;
        let meta_text = format!(
            "{}, {:.1}",
            format_relative_time(entry.entry().mtime_secs),
            entry.score()
        );

        let max_name_width = width.saturating_sub(prefix_width).saturating_sub(1);
        let display_rendered = if plain_name.len() > max_name_width && max_name_width > 2 {
            format!("{}…", truncate_with_ansi(&rendered_name, max_name_width.saturating_sub(1)))
        } else {
            rendered_name
        };

        line.left_mut().write_str(&display_rendered);
        line.right_mut().write_str(&if is_selected {
            meta_text
        } else {
            text::dim(&meta_text)
        });
    }

    fn render_create_line(&self, screen: &mut tui::Screen, is_selected: bool, _width: usize) {
        let background = if is_selected {
            Some(format!("{}{}", palette::selected_bg(), palette::selected_fg()))
        } else {
            None
        };
        let line = screen.body.add_line(background);
        if is_selected {
            line.left_mut().write_str(&text::highlight("→ "));
            line.left_mut().write_str(&self.selected_foreground());
        } else {
            line.left_mut().write_str("  ");
        }
        let date_prefix = crate::date::today_date_prefix();
        let label = if self.search.text.is_empty() {
            format!("📂 Create new: {}-", date_prefix)
        } else {
            format!("📂 Create new: {}-{}", date_prefix, self.search.text)
        };
        line.left_mut().write_str(&label);
    }

    fn selected_foreground(&self) -> String {
        if crate::ansi::colors_enabled() {
            palette::selected_fg()
        } else {
            String::new()
        }
    }

    fn formatted_entry_name(
        &self,
        entry: &fuzzy::MatchResult,
        selected: bool,
    ) -> (String, String) {
        let basename = &entry.entry().text;
        let positions = entry.positions();

        // Check for date prefix: ^(\d{4}-\d{2}-\d{2})-(.+)$
        if basename.len() > 11
            && basename.as_bytes()[4] == b'-'
            && basename.as_bytes()[7] == b'-'
            && basename.as_bytes()[10] == b'-'
            && basename.as_bytes()[0..4].iter().all(|b| b.is_ascii_digit())
            && basename.as_bytes()[5..7].iter().all(|b| b.is_ascii_digit())
            && basename.as_bytes()[8..10].iter().all(|b| b.is_ascii_digit())
        {
            let date_part = &basename[..10];
            let name_part = &basename[11..];
            let date_len = 11; // 10 + 1 for hyphen

            let mut rendered = if selected {
                date_part.to_string()
            } else {
                text::dim(date_part)
            };

            // Highlight hyphen if it's in positions
            let pos_set: HashSet<usize> = positions.iter().cloned().collect();
            let hyphen = if pos_set.contains(&10) {
                text::highlight("-")
            } else if selected {
                "-".to_string()
            } else {
                text::dim("-")
            };
            rendered.push_str(&hyphen);
            if selected && pos_set.contains(&10) {
                rendered.push_str(&self.selected_foreground());
            }
            rendered.push_str(&highlight_with_positions(
                name_part,
                &pos_set,
                date_len,
                selected,
                &self.selected_foreground(),
            ));

            (basename.clone(), rendered)
        } else {
            let pos_set: HashSet<usize> = positions.iter().cloned().collect();
            (
                basename.clone(),
                highlight_with_positions(basename, &pos_set, 0, selected, &self.selected_foreground()),
            )
        }
    }

    fn handle_selection(&mut self, try_dir: &fuzzy::MatchResult) {
        self.selected = Some(Selection::Cd {
            path: try_dir.entry().path.clone(),
        });
    }

    fn handle_create_new(&mut self) {
        let date_prefix = crate::date::today_date_prefix();

        if !self.search.text.is_empty() {
            let final_name = format!("{}-{}", date_prefix, self.search.text)
                .replace(char::is_whitespace, "-");
            let full_path = Path::new(&self.base_path).join(&final_name);
            self.selected = Some(Selection::Mkdir {
                path: full_path.to_string_lossy().to_string(),
            });
        } else {
            // No name typed — prompt for one
            // (In test mode, this won't be reached because show_create_new is false)
            self.clear_screen();
            let _ = write!(std::io::stderr(), "\x1b[?25h");
            let _ = writeln!(std::io::stderr(), "Enter new try name");
            let _ = writeln!(std::io::stderr());
            let _ = write!(std::io::stderr(), "> {}-", date_prefix);
            let _ = std::io::stderr().flush();

            let saved = term::enable_cooked_mode();
            term::stdin_iflush();
            let mut entry = String::new();
            let _ = std::io::stdin().read_line(&mut entry);
            if let Some(state) = saved {
                term::stty_set(&state);
            }

            let entry = entry.trim().to_string();
            if entry.is_empty() {
                return;
            }

            let final_name = format!("{}-{}", date_prefix, entry)
                .replace(char::is_whitespace, "-");
            let full_path = Path::new(&self.base_path).join(&final_name);
            self.selected = Some(Selection::Mkdir {
                path: full_path.to_string_lossy().to_string(),
            });
        }
    }

    fn confirm_batch_delete(&mut self, tries: &[fuzzy::MatchResult]) {
        let marked_items: Vec<fuzzy::MatchResult> = tries
            .iter()
            .filter(|t| self.marked_for_deletion.contains(&t.entry().path))
            .cloned()
            .map(|r| fuzzy::MatchResult::from(&r))
            .collect();

        if marked_items.is_empty() {
            return;
        }

        let mut input = tui::InputField::new("", "", None);

        if !self.test_keys.is_empty() {
            while !self.test_keys.is_empty() {
                let ch = self.test_keys.remove(0);
                if ch == "\r" || ch == "\n" {
                    break;
                }
                input.handle_key(&ch);
            }
            self.process_delete_confirmation(&marked_items, &input.text);
            return;
        } else if self.test_confirm.is_some() || !term::is_stderr_tty() {
            let confirmation = self.test_confirm.clone().unwrap_or_else(|| {
                let mut buf = String::new();
                let _ = std::io::stdin().read_line(&mut buf);
                buf.trim().to_string()
            });
            self.process_delete_confirmation(&marked_items, &confirmation);
            return;
        }

        // Interactive delete confirmation dialog
        self.clear_screen();
        loop {
            self.render_delete_dialog(&marked_items, &input.text, input.cursor);
            let ch = match self.read_key() {
                Some(k) => k,
                None => continue,
            };
            if input.handle_key(&ch) {
                continue;
            }
            match ch.as_str() {
                "\r" => {
                    self.process_delete_confirmation(&marked_items, &input.text);
                    break;
                }
                "\x1b" | "\x03" => {
                    self.delete_status = Some("Delete cancelled".to_string());
                    self.marked_for_deletion.clear();
                    self.delete_mode = false;
                    break;
                }
                _ => {}
            }
        }
        self.needs_redraw = true;
    }

    fn render_delete_dialog(
        &self,
        marked_items: &[fuzzy::MatchResult],
        confirmation_buffer: &str,
        confirmation_cursor: usize,
    ) {
        let mut screen = tui::Screen::new();

        let count = marked_items.len();
        let line = screen.header.add_line(None);
        line.left_mut().write_str(&emoji("🗑️"));
        line.left_mut().write_str(&text::accent(&format!(
            "  Delete {} {}?",
            count,
            if count == 1 { "directory" } else { "directories" }
        )));

        let line = screen.header.add_line(None);
        line.left_mut().write_fill(&tui::fill("─"));

        for item in marked_items {
            let line = screen.body.add_line(Some(palette::danger_bg()));
            line.left_mut().write_str(&emoji("🗑️"));
            line.left_mut().write_str(&format!(" {}", item.entry().text));
        }

        screen.body.add_line(None);
        screen.body.add_line(None);
        let line = screen.body.add_line(None);
        let prefix = "Type YES to confirm: ";
        line.center_mut().write_dim(prefix);
        let input_field = tui::InputField::new("", confirmation_buffer, Some(confirmation_cursor));
        line.center_mut().write_str(&input_field.render());
        let input_width = confirmation_buffer.len().max(confirmation_cursor + 1);
        let prefix_width = metrics::visible_width(prefix);
        let max_content = screen.width.saturating_sub(1);
        let center_start = (max_content.saturating_sub(prefix_width).saturating_sub(input_width)) / 2;
        line.mark_has_input(center_start + prefix_width);

        let line = screen.footer.add_line(None);
        line.left_mut().write_fill(&tui::fill("─"));
        let line = screen.footer.add_line(None);
        line.center_mut().write_dim("Enter: Confirm  Esc: Cancel");

        screen.render_to_stderr(Some((confirmation_cursor, 0)));
    }

    fn process_delete_confirmation(
        &mut self,
        marked_items: &[fuzzy::MatchResult],
        confirmation: &str,
    ) {
        if confirmation == "YES" {
            // Validate all paths first
            let base_real = std::fs::canonicalize(&self.base_path)
                .unwrap_or_else(|_| PathBuf::from(&self.base_path));
            let base_real_str = base_real.to_string_lossy().to_string();

            let mut validated_paths = Vec::new();
            let mut error = None;
            for item in marked_items {
                let target_real = match std::fs::canonicalize(&item.entry().path) {
                    Ok(p) => p,
                    Err(_) => {
                        error = Some(format!("Cannot resolve: {}", item.entry().path));
                        break;
                    }
                };
                let target_str = target_real.to_string_lossy().to_string();
                let prefix = format!("{}/", base_real_str);
                if !target_str.starts_with(&prefix) {
                    error = Some(format!(
                        "Safety check failed: {} is not inside {}",
                        target_str, base_real_str
                    ));
                    break;
                }
                validated_paths.push((target_str, item.entry().text.clone()));
            }

            if let Some(e) = error {
                self.delete_status = Some(format!("Error: {}", e));
                return;
            }

            let names: Vec<String> = validated_paths.iter().map(|(_, n)| n.clone()).collect();
            self.selected = Some(Selection::Delete {
                paths: validated_paths,
                base_path: base_real_str,
            });
            self.delete_status = Some(format!("Deleted: {}", names.join(", ")));
            self.all_tries = None;
            self.cached_results.clear();
            self.cached_query.clear();
            self.marked_for_deletion.clear();
            self.delete_mode = false;
        } else {
            self.delete_status = Some("Delete cancelled".to_string());
            self.marked_for_deletion.clear();
            self.delete_mode = false;
        }
    }

    fn run_rename_dialog(&mut self, entry: &DirEntry) {
        self.delete_mode = false;
        self.marked_for_deletion.clear();

        let current_name = entry.text.clone();
        let mut input = tui::InputField::new("", &current_name, None);
        let mut rename_error: Option<String> = None;

        loop {
            self.render_rename_dialog(&current_name, &input.text, input.cursor, &rename_error);
            let ch = match self.read_key() {
                Some(k) => k,
                None => continue,
            };
            let before = input.text.clone();
            if input.handle_key(&ch) {
                if input.text != before {
                    rename_error = None;
                }
                continue;
            }
            match ch.as_str() {
                "\r" => {
                    let result = self.finalize_rename(entry, &input.text);
                    if result.is_ok() {
                        break;
                    } else {
                        rename_error = result.err();
                    }
                }
                "\x1b" | "\x03" => break,
                _ => {}
            }
        }
        self.needs_redraw = true;
    }

    fn render_rename_dialog(
        &self,
        current_name: &str,
        rename_buffer: &str,
        rename_cursor: usize,
        rename_error: &Option<String>,
    ) {
        let mut screen = tui::Screen::new();

        let line = screen.header.add_line(None);
        line.left_mut().write_str(&emoji("✏️"));
        line.left_mut().write_str(&text::accent("  Rename directory"));

        let line = screen.header.add_line(None);
        line.left_mut().write_fill(&tui::fill("─"));

        let line = screen.body.add_line(None);
        line.left_mut().write_str(&emoji("📁"));
        line.left_mut().write_str(&format!(" {}", current_name));

        screen.body.add_line(None);
        screen.body.add_line(None);
        let line = screen.body.add_line(None);
        let prefix = "New name: ";
        line.center_mut().write_dim(prefix);
        let input_field = tui::InputField::new("", rename_buffer, Some(rename_cursor));
        line.center_mut().write_str(&input_field.render());
        let input_width = rename_buffer.len().max(rename_cursor + 1);
        let prefix_width = metrics::visible_width(prefix);
        let max_content = screen.width.saturating_sub(1);
        let center_start = (max_content.saturating_sub(prefix_width).saturating_sub(input_width)) / 2;
        line.mark_has_input(center_start + prefix_width);

        if let Some(err) = rename_error {
            screen.body.add_line(None);
            let line = screen.body.add_line(None);
            line.center_mut().write_bold(err);
        }

        let line = screen.footer.add_line(None);
        line.left_mut().write_fill(&tui::fill("─"));
        let line = screen.footer.add_line(None);
        line.center_mut().write_dim("Enter: Confirm  Esc: Cancel");

        screen.render_to_stderr(Some((rename_cursor, 0)));
    }

    fn finalize_rename(&mut self, entry: &DirEntry, rename_buffer: &str) -> Result<(), String> {
        let new_name = rename_buffer
            .trim()
            .replace(char::is_whitespace, "-");

        if new_name.is_empty() {
            return Err("Name cannot be empty".to_string());
        }
        if new_name.contains('/') {
            return Err("Name cannot contain /".to_string());
        }
        if new_name == entry.text {
            return Ok(());
        }
        let new_path = Path::new(&self.base_path).join(&new_name);
        if new_path.exists() {
            return Err(format!("Directory exists: {}", new_name));
        }

        self.selected = Some(Selection::Rename {
            old: entry.text.clone(),
            new: new_name,
            base_path: self.base_path.clone(),
        });
        Ok(())
    }

    fn run_ascend_dialog(&mut self, entry: &DirEntry) {
        self.delete_mode = false;
        self.marked_for_deletion.clear();

        let current_name = entry.text.clone();
        let project_name = if current_name.len() > 11 {
            let prefix = &current_name[..11];
            if prefix.as_bytes()[4] == b'-'
                && prefix.as_bytes()[7] == b'-'
                && prefix.as_bytes()[10] == b'-'
            {
                &current_name[11..]
            } else {
                &current_name
            }
        } else {
            &current_name
        };

        let projects_dir = if let Some(tp) = try_projects() {
            crate::shell::expand_tilde(&tp)
        } else {
            Path::new(&self.base_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| self.base_path.clone())
        };

        let default_dest = Path::new(&projects_dir).join(project_name);
        let mut input = tui::InputField::new(
            "",
            &default_dest.to_string_lossy(),
            None,
        );
        let mut ascend_error: Option<String> = None;

        loop {
            self.render_ascend_dialog(
                &current_name,
                &input.text,
                input.cursor,
                &ascend_error,
                &projects_dir,
            );
            let ch = match self.read_key() {
                Some(k) => k,
                None => continue,
            };
            let before = input.text.clone();
            if input.handle_key(&ch) {
                if input.text != before {
                    ascend_error = None;
                }
                continue;
            }
            match ch.as_str() {
                "\r" => {
                    let result = self.finalize_ascend(entry, &input.text);
                    if result.is_ok() {
                        break;
                    } else {
                        ascend_error = result.err();
                    }
                }
                "\x1b" | "\x03" => break,
                _ => {}
            }
        }
        self.needs_redraw = true;
    }

    fn render_ascend_dialog(
        &self,
        current_name: &str,
        ascend_buffer: &str,
        ascend_cursor: usize,
        ascend_error: &Option<String>,
        projects_dir: &str,
    ) {
        let mut screen = tui::Screen::new();

        let line = screen.header.add_line(None);
        line.left_mut().write_str(&emoji("🚀"));
        line.left_mut().write_str(&text::accent("  Graduate try to project"));

        let line = screen.header.add_line(None);
        line.left_mut().write_fill(&tui::fill("─"));

        let line = screen.body.add_line(None);
        line.left_mut().write_str(&emoji("📁"));
        line.left_mut().write_str(&format!(" {}", current_name));
        screen.body.add_line(None);

        let env_hint = if try_projects().is_some() {
            "$TRY_PROJECTS"
        } else {
            "parent of $TRY_PATH"
        };
        let line = screen.body.add_line(None);
        line.center_mut().write_dim(&format!("Destination ({}: {})", env_hint, projects_dir));

        screen.body.add_line(None);
        let line = screen.body.add_line(None);
        let prefix = "Move to: ";
        line.center_mut().write_dim(prefix);
        let input_field = tui::InputField::new("", ascend_buffer, Some(ascend_cursor));
        line.center_mut().write_str(&input_field.render());
        let input_width = ascend_buffer.len().max(ascend_cursor + 1);
        let prefix_width = metrics::visible_width(prefix);
        let max_content = screen.width.saturating_sub(1);
        let center_start = (max_content.saturating_sub(prefix_width).saturating_sub(input_width)) / 2;
        line.mark_has_input(center_start + prefix_width);

        screen.body.add_line(None);
        let line = screen.body.add_line(None);
        line.center_mut().write_dim("A symlink will be left in the tries directory");

        if let Some(err) = ascend_error {
            screen.body.add_line(None);
            let line = screen.body.add_line(None);
            line.center_mut().write_bold(err);
        }

        let line = screen.footer.add_line(None);
        line.left_mut().write_fill(&tui::fill("─"));
        let line = screen.footer.add_line(None);
        line.center_mut().write_dim("Enter: Confirm  Esc: Cancel");

        screen.render_to_stderr(Some((ascend_cursor, 0)));
    }

    fn finalize_ascend(&mut self, entry: &DirEntry, ascend_buffer: &str) -> Result<(), String> {
        let dest = ascend_buffer.trim();
        let dest = crate::shell::expand_tilde(dest);
        let dest_path = PathBuf::from(&dest);

        if dest.is_empty() {
            return Err("Destination cannot be empty".to_string());
        }
        if dest_path.exists() {
            return Err(format!("Destination already exists: {}", dest));
        }

        let parent = dest_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if !Path::new(&parent).is_dir() {
            return Err(format!("Parent directory does not exist: {}", parent));
        }

        self.selected = Some(Selection::Ascend {
            source: entry.path.clone(),
            dest,
            basename: entry.text.clone(),
            base_path: self.base_path.clone(),
        });
        Ok(())
    }
}

/// Highlight matched characters in text.
fn highlight_with_positions(
    text_str: &str,
    pos_set: &HashSet<usize>,
    offset: usize,
    selected: bool,
    selected_fg: &str,
) -> String {
    let chars: Vec<char> = text_str.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        if pos_set.contains(&(i + offset)) {
            // Batch consecutive highlighted characters
            let batch_start = i;
            i += 1;
            while i < chars.len() && pos_set.contains(&(i + offset)) {
                i += 1;
            }
            result.push_str(&crate::ansi::text::highlight(
                &chars[batch_start..i].iter().collect::<String>(),
            ));
            if selected {
                result.push_str(selected_fg);
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Truncate text with ANSI codes, preserving escape sequences.
fn truncate_with_ansi(text: &str, max_length: usize) -> String {
    let mut visible_count = 0;
    let mut result = String::new();
    let mut in_ansi = false;

    for char in text.chars() {
        if char == '\x1b' {
            in_ansi = true;
            result.push(char);
        } else if in_ansi {
            result.push(char);
            if char.is_ascii_alphabetic() {
                in_ansi = false;
            }
        } else {
            if visible_count >= max_length {
                break;
            }
            result.push(char);
            visible_count += 1;
        }
    }
    result
}

/// Format relative time from seconds-since-epoch.
fn format_relative_time(mtime_secs: f64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let seconds = now - mtime_secs;
    let minutes = seconds / 60.0;
    let hours = minutes / 60.0;
    let days = hours / 24.0;

    if seconds < 60.0 {
        "just now".to_string()
    } else if minutes < 60.0 {
        format!("{}m ago", minutes as i64)
    } else if hours < 24.0 {
        format!("{}h ago", hours as i64)
    } else if days < 7.0 {
        format!("{}d ago", days as i64)
    } else {
        format!("{}w ago", (days / 7.0) as i64)
    }
}

/// Helper to get emoji string for rendering (triggers wide-char handling).
fn emoji(ch: &str) -> String {
    ch.to_string()
}

// FdSet for poll()
#[repr(C)]
#[derive(Default)]
struct FdSet {
    fd: i32,
    events: i16,
    revents: i16,
}


#[allow(non_camel_case_types)]
type PollFlagsType = i16;

#[allow(non_upper_case_globals)]
const POLLIN: PollFlagsType = 0x001;
