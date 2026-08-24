//! Terminal control — raw mode via stty (mirrors TryCompat in try.rb).

use std::process::Command;

/// Save the current stty state.
pub fn stty_save() -> String {
    let output = Command::new("sh")
        .args(["-c", "stty -g 2>/dev/null"])
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

/// Flush stdin (discard pending input) — best effort.
pub fn stdin_iflush() {
    #[cfg(unix)]
    {
        use std::io::Read;
        use std::os::fd::AsRawFd;
        let fd = std::io::stdin().as_raw_fd();
        let flags = unsafe { ffi::fcntl(fd, F_GETFL, 0) };
        if flags < 0 {
            return;
        }
        unsafe {
            ffi::fcntl(fd, F_SETFL, flags | O_NONBLOCK);
        }
        let mut buf = [0u8; 4096];
        loop {
            match std::io::stdin().read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => continue,
            }
        }
        unsafe {
            ffi::fcntl(fd, F_SETFL, flags);
        }
    }
}

#[cfg(unix)]
const F_GETFL: i32 = 3;
#[cfg(unix)]
const F_SETFL: i32 = 4;
#[cfg(unix)]
const O_NONBLOCK: i32 = 0o4000;

#[cfg(unix)]
mod ffi {
    extern "C" {
        pub fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    }
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
    use std::io::Read;
    let mut input = [0u8; 1];
    std::io::stdin().read_exact(&mut input).ok()?;
    let input = input[0];

    if input == 0x1b {
        // Escape sequence — try to read more
        let mut result = String::from("\x1b");
        if let Some(nxt) = read_nonblock_byte() {
            result.push(nxt);
            if nxt == '[' {
                // CSI: consume until a final byte in 0x40-0x7E
                while let Some(ch) = read_nonblock_byte() {
                    result.push(ch);
                    let code = ch as u32;
                    if (0x40..=0x7E).contains(&code) {
                        break;
                    }
                }
                // X10 mouse: ESC [ M + 3 payload bytes
                if result == "\x1b[M" {
                    for _ in 0..3 {
                        if let Some(ch) = read_nonblock_byte() {
                            result.push(ch);
                        }
                    }
                }
            } else if nxt == 'O' {
                if let Some(ch) = read_nonblock_byte() {
                    result.push(ch);
                }
            }
        }
        return Some(result);
    }

    // Single byte — handle UTF-8 continuation
    Some((input as char).to_string())
}

/// Read a byte from stdin without blocking (returns None if no data).
fn read_nonblock_byte() -> Option<char> {
    use std::io::Read;
    use std::os::fd::AsRawFd;

    #[cfg(unix)]
    {
        let fd = std::io::stdin().as_raw_fd();
        let flags = unsafe { ffi::fcntl(fd, F_GETFL, 0) };
        if flags < 0 {
            return None;
        }
        unsafe {
            ffi::fcntl(fd, F_SETFL, flags | O_NONBLOCK);
        }
        let mut buf = [0u8; 1];
        let result = std::io::stdin().read(&mut buf).ok().filter(|n| *n > 0);
        unsafe {
            ffi::fcntl(fd, F_SETFL, flags);
        }
        result.map(|_| buf[0] as char)
    }
    #[cfg(not(unix))]
    {
        None
    }
}
