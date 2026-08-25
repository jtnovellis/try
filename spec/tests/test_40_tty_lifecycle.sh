# Real-terminal lifecycle tests: raw-mode restore and stdin readiness.
# Spec: tui_spec.md (Terminal Lifecycle)
#
# Regression coverage for two macOS-specific hangs:
#   1. `stty_save` ran `stty -g` with stdin on /dev/null, so it returned an
#      empty state and the terminal was never taken back out of raw mode
#      (ISIG stayed off — Ctrl-C could no longer signal the process).
#   2. `O_NONBLOCK` was hardcoded to the Linux value (0o4000), so every
#      "non-blocking" stdin read blocked forever — ESC froze the main loop and
#      every exit path hung in the stdin flush.
#
# These need a real pty: the --and-keys hooks bypass the terminal entirely.
# Driven through spec/tests/pty_drive.py (uses $TRY_BIN_PATH directly, so
# command wrappers like valgrind do not apply here).

section "tty-lifecycle"

PTY_DRIVE="$SPEC_DIR/tests/pty_drive.py"

if ! command -v python3 >/dev/null 2>&1 || [ ! -f "$PTY_DRIVE" ]; then
    echo -n " (skipped: python3 required)"
else

# Run try in a pty and echo the driver's report.
# Usage: pty_run <keys> [timeout]
pty_run() {
    python3 "$PTY_DRIVE" --keys "$1" --timeout "${2:-8}" -- \
        "$TRY_BIN_PATH" exec --path "$TEST_TRIES" 2>&1
}

# BSD sed does not understand \x1b, so build the escape byte with printf.
ESC=$(printf '\033')

pty_output() {
    echo "$1" | sed -n 's/^OUTPUT_B64=//p' | base64 -d 2>/dev/null
}

# --- ESC cancels and the process actually exits -----------------------------
report=$(pty_run '\x1b')
if echo "$report" | grep -q '^TIMED_OUT=0$'; then
    pass
else
    fail "ESC must exit the selector" "TIMED_OUT=0" "$report" "tui_spec.md#terminal-lifecycle"
fi

if echo "$report" | grep -q '^EXIT_CODE=1$'; then
    pass
else
    fail "ESC should cancel with exit code 1" "EXIT_CODE=1" "$report" "tui_spec.md#cancel-behavior"
fi

if pty_output "$report" | grep -q "Cancelled"; then
    pass
else
    fail "ESC should report cancellation" "Cancelled." "$(pty_output "$report")" "tui_spec.md#cancel-behavior"
fi

# --- ESC must leave the terminal usable -------------------------------------
if echo "$report" | grep -q '^TTY_RESTORED=1$'; then
    pass
else
    fail "terminal must leave raw mode on cancel" "TTY_RESTORED=1" "$report" "tui_spec.md#terminal-lifecycle"
fi

# --- Ctrl-C cancels and the process actually exits --------------------------
report=$(pty_run '\x03')
if echo "$report" | grep -q '^TIMED_OUT=0$'; then
    pass
else
    fail "Ctrl-C must exit the selector" "TIMED_OUT=0" "$report" "tui_spec.md#terminal-lifecycle"
fi

if echo "$report" | grep -q '^EXIT_CODE=1$'; then
    pass
else
    fail "Ctrl-C should cancel with exit code 1" "EXIT_CODE=1" "$report" "tui_spec.md#cancel-behavior"
fi

if echo "$report" | grep -q '^TTY_RESTORED=1$'; then
    pass
else
    fail "terminal must leave raw mode after Ctrl-C" "TTY_RESTORED=1" "$report" "tui_spec.md#terminal-lifecycle"
fi

# --- Enter selects, exits cleanly, and restores the terminal ----------------
report=$(pty_run '\r')
if echo "$report" | grep -q '^TIMED_OUT=0$'; then
    pass
else
    fail "Enter must exit the selector" "TIMED_OUT=0" "$report" "tui_spec.md#terminal-lifecycle"
fi

if echo "$report" | grep -q '^EXIT_CODE=0$'; then
    pass
else
    fail "Enter should succeed with exit code 0" "EXIT_CODE=0" "$report" "tui_spec.md#enter-action"
fi

if pty_output "$report" | grep -q "cd "; then
    pass
else
    fail "Enter should emit a cd script" "cd command" "$(pty_output "$report")" "tui_spec.md#enter-action"
fi

if echo "$report" | grep -q '^TTY_RESTORED=1$'; then
    pass
else
    fail "terminal must leave raw mode after selecting" "TTY_RESTORED=1" "$report" "tui_spec.md#terminal-lifecycle"
fi

# --- Escape sequences still parse as arrow keys, not bare ESC ---------------
# Down-arrow is ESC [ B: if the reader treated the ESC as standalone it would
# cancel instead of moving the cursor.
first=$(pty_output "$(pty_run '\r')" | grep -o "cd '[^']*'" | head -1)
second=$(pty_output "$(pty_run '\x1b[B\r')" | grep -o "cd '[^']*'" | head -1)
if [ -n "$second" ] && [ "$first" != "$second" ]; then
    pass
else
    fail "down arrow should move the cursor before Enter" "a different entry than $first" "$second" "tui_spec.md#navigation"
fi

# --- Typing still filters, and Ctrl-C exits from a filtered list ------------
report=$(pty_run 'alpha\r')
if echo "$report" | grep -q '^EXIT_CODE=0$' && pty_output "$report" | grep -q "alpha"; then
    pass
else
    fail "typing should filter then select" "cd to the alpha entry" "$(pty_output "$report")" "fuzzy_matching.md"
fi

# --- Non-ASCII keys reach the search field ---------------------------------
# A UTF-8 character arrives as several bytes. Handling only the first one turns
# every accented letter, CJK character and emoji into a dropped keystroke.
# (--and-keys cannot express these: it splits raw input per byte.)
search_line() {
    pty_output "$1" \
        | sed "s/${ESC}\[[0-9;?]*[a-zA-Z]//g" \
        | tr '\r' '\n' \
        | grep '^Search:' | tail -1
}

for pair in 'café:caf\xc3\xa9' 'ñ:\xc3\xb1' '日本:\xe6\x97\xa5\xe6\x9c\xac'; do
    want="${pair%%:*}"
    keys="${pair#*:}"
    line=$(search_line "$(pty_run "$keys" 5)")
    if echo "$line" | grep -q "$want"; then
        pass
    else
        fail "typing '$want' should appear in the search field" "Search: $want" "$line" "tui_spec.md#terminal-lifecycle"
    fi
done

# --- Resizing the window re-renders without waiting for a keypress ---------
# Spec: tui_spec.md "Resize Handling" — query the new size, re-render, and keep
# the selection where it was.
# TRY_WIDTH/TRY_HEIGHT pin the size and would mask the ioctl entirely, so this
# one scenario has to run without them.
resize_report=$(env -u TRY_WIDTH -u TRY_HEIGHT python3 "$PTY_DRIVE" \
    --pre-keys '\x1b[B' --resize 24x40 --keys '\r' \
    --timeout 10 -- "$TRY_BIN_PATH" exec --path "$TEST_TRIES" 2>&1)
bytes_after=$(echo "$resize_report" | sed -n 's/^BYTES_AFTER_RESIZE=//p')
if [ -n "$bytes_after" ] && [ "$bytes_after" -gt 0 ]; then
    pass
else
    fail "resize should redraw without a keypress" "BYTES_AFTER_RESIZE > 0" "$resize_report" "tui_spec.md#resize-handling"
fi

# Selection must survive the resize: down-arrow, resize, Enter picks the same
# entry as down-arrow, Enter at a fixed size.
resized_pick=$(pty_output "$resize_report" | sed -n "s/.*cd '\([^']*\)'.*/\1/p" | head -1)
steady_pick=$(pty_output "$(env -u TRY_WIDTH -u TRY_HEIGHT python3 "$PTY_DRIVE" --keys '\x1b[B\r' --timeout 8 -- "$TRY_BIN_PATH" exec --path "$TEST_TRIES" 2>&1)" | sed -n "s/.*cd '\([^']*\)'.*/\1/p" | head -1)
if [ -n "$resized_pick" ] && [ "$resized_pick" = "$steady_pick" ]; then
    pass
else
    fail "resize should preserve the selection" "$steady_pick" "$resized_pick" "tui_spec.md#resize-handling"
fi

# The narrower layout must actually be used: the redraw the resize triggered
# must fit 40 columns (measured in characters by the driver, since the box
# drawing glyphs are multi-byte).
widest=$(echo "$resize_report" | sed -n 's/^WIDEST_AFTER_RESIZE=//p')
if [ -n "$widest" ] && [ "$widest" -gt 0 ] && [ "$widest" -le 40 ]; then
    pass
else
    fail "resized render should fit 40 columns" "<= 40" "widest line: $widest" "tui_spec.md#resize-handling"
fi

# A multi-byte character must also survive into a created directory name.
report=$(pty_run 'caf\xc3\xa9\r')
if pty_output "$report" | grep -q "café"; then
    pass
else
    fail "non-ASCII query should reach the created path" "café in the script" "$(pty_output "$report")" "tui_spec.md#new-directory-creation"
fi

fi
