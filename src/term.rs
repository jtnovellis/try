//! Terminal control — raw mode via stty (mirrors TryCompat in try.rb).

use std::process::{Command, Stdio};

/// How long to wait for the rest of an escape sequence before treating a lone
/// ESC as a standalone keypress.
const ESC_SEQUENCE_TIMEOUT_MS: i32 = 50;

/// Save the current stty state.
pub fn stty_save() -> String {
    // `stty` reads the terminal from its *stdin*. `Command::output()` defaults
    // stdin to /dev/null, which makes `stty -g` fail with "stdin isn't a
    // terminal" and silently yield an empty state — leaving the terminal stuck
    // in raw mode (no ISIG, so Ctrl-C stops working). Inherit the real tty.
    let output = Command::new("sh")
        .args(["-c", "stty -g 2>/dev/null"])
        .stdin(Stdio::inherit())
        .output();
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

/// Restore stty to a saved state.
pub fn stty_set(state: &str) {
    if state.is_empty() {
        return;
    }
    let _ = Command::new("sh")
        .args(["-c", &format!("stty {} 2>/dev/null", state)])
        .status();
}

/// Enter raw mode (no echo, raw input).
pub fn enable_raw_mode() -> Option<String> {
    if !is_stdin_tty() {
        return None;
    }
    let saved = stty_save();
    let _ = Command::new("sh")
        .args(["-c", "stty raw -echo 2>/dev/null"])
        .status();
    Some(saved)
}

/// Enter cooked mode (echo, normal input).
pub fn enable_cooked_mode() -> Option<String> {
    if !is_stdin_tty() {
        return None;
    }
    let saved = stty_save();
    let _ = Command::new("sh")
        .args(["-c", "stty cooked echo 2>/dev/null"])
        .status();
    Some(saved)
}

#[cfg(unix)]
mod sys {
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct PollFd {
        pub fd: i32,
        pub events: i16,
        pub revents: i16,
    }

    pub const POLLIN: i16 = 0x001;

    // nfds_t is `unsigned long` on Linux and `unsigned int` on BSD/macOS.
    #[cfg(target_os = "linux")]
    pub type NfdsT = std::os::raw::c_ulong;
    #[cfg(not(target_os = "linux"))]
    pub type NfdsT = std::os::raw::c_uint;

    extern "C" {
        pub fn poll(fds: *mut PollFd, nfds: NfdsT, timeout: i32) -> i32;
        pub fn read(fd: i32, buf: *mut u8, count: usize) -> isize;
    }
}

/// The raw file descriptor for stdin.
pub fn stdin_fd() -> i32 {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        std::io::stdin().as_raw_fd()
    }
    #[cfg(not(unix))]
    {
        0
    }
}

/// Whether `fd` has data available to read.
///
/// `timeout_ms` is the wait budget: `0` polls, a negative value blocks until
/// data arrives. Readiness is asked of the kernel via `poll(2)` rather than a
/// non-blocking `read` — the `O_NONBLOCK` bit differs per platform (0x4 on
/// macOS/BSD, 0o4000 on Linux) and getting it wrong turns every "non-blocking"
/// read into a permanent hang.
#[cfg(unix)]
pub fn fd_ready(fd: i32, timeout_ms: i32) -> bool {
    let mut fds = [sys::PollFd {
        fd,
        events: sys::POLLIN,
        revents: 0,
    }];
    loop {
        let ret = unsafe { sys::poll(fds.as_mut_ptr(), 1, timeout_ms) };
        if ret < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return false;
        }
        return ret > 0 && (fds[0].revents & sys::POLLIN) != 0;
    }
}

#[cfg(not(unix))]
pub fn fd_ready(_fd: i32, _timeout_ms: i32) -> bool {
    false
}

/// Whether stdin has data available to read.
pub fn stdin_ready(timeout_ms: i32) -> bool {
    fd_ready(stdin_fd(), timeout_ms)
}

/// Read up to `buf.len()` bytes straight from `fd`. Returns the byte count, or
/// `None` on error.
///
/// This deliberately bypasses `std::io::Stdin`'s internal `BufReader`, which
/// would pull bytes out of the kernel into a userspace buffer that `poll()`
/// cannot see — making every subsequent readiness check lie.
#[cfg(unix)]
fn read_fd(fd: i32, buf: &mut [u8]) -> Option<usize> {
    loop {
        let n = unsafe { sys::read(fd, buf.as_mut_ptr(), buf.len()) };
        if n < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return None;
        }
        return Some(n as usize);
    }
}

#[cfg(not(unix))]
fn read_fd(_fd: i32, buf: &mut [u8]) -> Option<usize> {
    use std::io::Read;
    std::io::stdin().read(buf).ok()
}

/// Discard any pending input on `fd` without blocking.
pub fn drain_fd(fd: i32) {
    let mut buf = [0u8; 4096];
    while fd_ready(fd, 0) {
        match read_fd(fd, &mut buf) {
            Some(n) if n > 0 => continue,
            _ => break,
        }
    }
}

/// Flush stdin (discard pending input) — best effort.
pub fn stdin_iflush() {
    drain_fd(stdin_fd());
}

pub fn is_stdin_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

pub fn is_stderr_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}

/// Read a single keypress from stdin (handles escape sequences).
pub fn read_keypress() -> Option<String> {
    let mut input = [0u8; 1];
    if read_fd(stdin_fd(), &mut input)? != 1 {
        return None;
    }
    let input = input[0];

    if input == 0x1b {
        // Escape sequence — try to read more
        let mut result = String::from("\x1b");
        if let Some(nxt) = read_pending_byte().map(char::from) {
            result.push(nxt);
            if nxt == '[' {
                // CSI: consume until a final byte in 0x40-0x7E
                while let Some(ch) = read_pending_byte().map(char::from) {
                    result.push(ch);
                    let code = ch as u32;
                    if (0x40..=0x7E).contains(&code) {
                        break;
                    }
                }
                // X10 mouse: ESC [ M + 3 payload bytes
                if result == "\x1b[M" {
                    for _ in 0..3 {
                        if let Some(ch) = read_pending_byte().map(char::from) {
                            result.push(ch);
                        }
                    }
                }
            } else if nxt == 'O' {
                if let Some(ch) = read_pending_byte().map(char::from) {
                    result.push(ch);
                }
            }
        }
        return Some(result);
    }

    // A UTF-8 scalar spans up to four bytes. The terminal delivers them
    // together, so gather the continuation bytes and return the whole
    // character — otherwise typing any non-ASCII key is silently dropped.
    let mut bytes = vec![input];
    for _ in 0..utf8_continuation_len(input) {
        match read_pending_byte() {
            Some(b) => bytes.push(b),
            None => break,
        }
    }
    String::from_utf8(bytes).ok()
}

/// How many continuation bytes follow a UTF-8 leading byte.
fn utf8_continuation_len(lead: u8) -> usize {
    match lead {
        0xc0..=0xdf => 1,
        0xe0..=0xef => 2,
        0xf0..=0xf7 => 3,
        _ => 0,
    }
}

/// Read the next byte of a multi-byte key, or `None` if nothing more arrives
/// within `ESC_SEQUENCE_TIMEOUT_MS` (a bare ESC, or a truncated sequence).
fn read_pending_byte() -> Option<u8> {
    let fd = stdin_fd();
    if !fd_ready(fd, ESC_SEQUENCE_TIMEOUT_MS) {
        return None;
    }
    let mut buf = [0u8; 1];
    match read_fd(fd, &mut buf) {
        Some(1) => Some(buf[0]),
        _ => None,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    /// Run `f` on a worker thread, failing the test if it does not finish.
    /// A readiness check that actually blocks would otherwise hang forever
    /// instead of reporting a failure.
    fn with_watchdog<F: FnOnce() + Send + 'static>(what: &str, f: F) {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            f();
            let _ = tx.send(());
        });
        assert!(
            rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "{what} blocked instead of returning"
        );
    }

    #[test]
    fn fd_ready_reports_no_data_on_an_idle_fd() {
        let (rx, _tx) = UnixStream::pair().unwrap();
        let fd = rx.as_raw_fd();
        with_watchdog("fd_ready on an idle fd", move || {
            assert!(!fd_ready(fd, 0), "idle fd must not report data");
        });
    }

    #[test]
    fn fd_ready_reports_data_once_it_arrives() {
        let (rx, mut tx) = UnixStream::pair().unwrap();
        let fd = rx.as_raw_fd();
        assert!(!fd_ready(fd, 0));
        tx.write_all(b"x").unwrap();
        assert!(fd_ready(fd, 200), "fd with a pending byte must report data");
    }

    #[test]
    fn fd_ready_honors_its_timeout_instead_of_blocking() {
        let (rx, _tx) = UnixStream::pair().unwrap();
        let fd = rx.as_raw_fd();
        with_watchdog("fd_ready with a finite timeout", move || {
            let start = std::time::Instant::now();
            assert!(!fd_ready(fd, 50));
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "fd_ready waited far longer than its 50ms budget"
            );
        });
    }

    #[test]
    fn drain_fd_discards_pending_input_without_blocking() {
        let (rx, mut tx) = UnixStream::pair().unwrap();
        let fd = rx.as_raw_fd();
        tx.write_all(b"leftover keystrokes").unwrap();
        with_watchdog("drain_fd", move || {
            drain_fd(fd);
            assert!(!fd_ready(fd, 0), "drain_fd must consume all pending input");
        });
    }

    #[test]
    fn drain_fd_returns_immediately_when_there_is_nothing_to_discard() {
        let (rx, _tx) = UnixStream::pair().unwrap();
        let fd = rx.as_raw_fd();
        with_watchdog("drain_fd on an idle fd", move || drain_fd(fd));
    }

    #[test]
    fn stty_set_ignores_an_empty_state() {
        // A failed `stty -g` yields "" — restoring that must be a no-op rather
        // than shelling out to a bare `stty`.
        stty_set("");
    }
}
