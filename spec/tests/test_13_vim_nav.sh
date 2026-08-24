# Vim-style navigation tests
# Spec: tui_spec.md (Keyboard Input)
# | ↑ / Ctrl-P / Ctrl-K | Move selection up |
# | ↓ / Ctrl-N / Ctrl-J | Move selection down |

section "vim-nav"

# The emitted script indents its cd line ("  cd '...' && \\"), so an anchored
# "^cd '" match finds nothing and any comparison built on it is vacuous.
selected_path() {
    grep "cd '" | sed -n "s/.*cd '\\([^']*\\)'.*/\\1/p" | head -1
}

# Test: Ctrl-J navigates down (vim-style)
output=$(try_run --path="$TEST_TRIES" --and-keys='CTRL-J,ENTER' exec 2>/dev/null)
if echo "$output" | grep -q "cd '"; then
    pass
else
    fail "Ctrl-J should navigate down" "cd command" "$output" "tui_spec.md#keyboard-input"
fi

# Test: Ctrl-K navigates up (vim-style)
output=$(try_run --path="$TEST_TRIES" --and-keys='CTRL-J,CTRL-K,ENTER' exec 2>/dev/null)
if echo "$output" | grep -q "cd '"; then
    pass
else
    fail "Ctrl-K should navigate up" "cd command" "$output" "tui_spec.md#keyboard-input"
fi

# Test: Ctrl-N navigates down (emacs-style)
output=$(try_run --path="$TEST_TRIES" --and-keys='CTRL-N,ENTER' exec 2>/dev/null)
if echo "$output" | grep -q "cd '"; then
    pass
else
    fail "Ctrl-N should navigate down" "cd command" "$output" "tui_spec.md#keyboard-input"
fi

# Test: Ctrl-P navigates up (emacs-style)
output=$(try_run --path="$TEST_TRIES" --and-keys='CTRL-N,CTRL-P,ENTER' exec 2>/dev/null)
if echo "$output" | grep -q "cd '"; then
    pass
else
    fail "Ctrl-P should navigate up" "cd command" "$output" "tui_spec.md#keyboard-input"
fi

# Test: Ctrl-J then Ctrl-K returns to same position
first=$(try_run --path="$TEST_TRIES" --and-keys='ENTER' exec 2>/dev/null | selected_path)
round_trip=$(try_run --path="$TEST_TRIES" --and-keys='CTRL-J,CTRL-K,ENTER' exec 2>/dev/null | selected_path)
if [ -n "$first" ] && [ "$first" = "$round_trip" ]; then
    pass
else
    fail "Ctrl-J then Ctrl-K should return to same item" "same cd path" "first: $first, round_trip: $round_trip" "tui_spec.md#keyboard-input"
fi

# Test: Ctrl-N then Ctrl-P returns to same position
first=$(try_run --path="$TEST_TRIES" --and-keys='ENTER' exec 2>/dev/null | selected_path)
round_trip=$(try_run --path="$TEST_TRIES" --and-keys='CTRL-N,CTRL-P,ENTER' exec 2>/dev/null | selected_path)
if [ -n "$first" ] && [ "$first" = "$round_trip" ]; then
    pass
else
    fail "Ctrl-N then Ctrl-P should return to same item" "same cd path" "first: $first, round_trip: $round_trip" "tui_spec.md#keyboard-input"
fi

# Test: Ctrl-K actually moves the cursor, rather than being swallowed by the
# search field's kill-to-end binding (they share the same 0x0b byte).
down=$(try_run --path="$TEST_TRIES" --and-keys='CTRL-J,ENTER' exec 2>/dev/null | selected_path)
back_up=$(try_run --path="$TEST_TRIES" --and-keys='CTRL-J,CTRL-K,ENTER' exec 2>/dev/null | selected_path)
top=$(try_run --path="$TEST_TRIES" --and-keys='ENTER' exec 2>/dev/null | selected_path)
if [ -n "$down" ] && [ "$down" != "$top" ] && [ "$back_up" = "$top" ]; then
    pass
else
    fail "Ctrl-K must navigate up, not kill to end of line" "back to $top" "down: $down, back_up: $back_up" "tui_spec.md#keyboard-input"
fi

# Test: Ctrl-K in the search field does not clear the query either
output=$(try_run --path="$TEST_TRIES" --and-keys='TYPE=beta,CTRL-K,ENTER' exec 2>/dev/null | selected_path)
if echo "$output" | grep -q "beta"; then
    pass
else
    fail "Ctrl-K should not discard the typed query" "a beta path" "$output" "tui_spec.md#keyboard-input"
fi
