# Styling System Specification

## Overview

The styling system applies ANSI escape sequences for text formatting. In the Rust implementation, styling is handled by functions in `src/ansi.rs` that wrap text with the appropriate escape codes directly — there is no intermediate token representation.

## Styling Functions

All styling functions are in `src/ansi.rs`. They respect the global `colors_enabled` flag, which is controlled by the `--no-colors` flag and the `NO_COLOR` environment variable.

### Text Formatting

| Function | Effect | Description |
|----------|--------|-------------|
| `text::bold(text)` | Bold + reset intensity | Bold text |
| `text::dim(text)` | 256-color gray (245) + reset fg | De-emphasized/secondary text |
| `text::highlight(text)` | Bold yellow + reset | Fuzzy match highlight |
| `text::accent(text)` | Bold orange (214) + reset | Headings, dialog titles |

### Color Palette

| Function | Effect | Description |
|----------|--------|-------------|
| `palette::selected_bg()` | 256-color bg (238) | Selected row background |
| `palette::selected_fg()` | 256-color fg (255) | Selected row foreground |
| `palette::danger_bg()` | 256-color bg (52) | Delete mode background |
| `palette::input_cursor_on()` | Reverse video (7m) | Input cursor |
| `palette::input_cursor_off()` | Reset reverse (27m) | End input cursor |

### ANSI Sequences

All sequences use standard SGR (Select Graphic Rendition) codes:

- Foreground: `\x1b[38;5;{code}m` (256-color)
- Background: `\x1b[48;5;{code}m` (256-color)
- Reset all: `\x1b[0m`
- Reset foreground: `\x1b[39m`
- Reset intensity: `\x1b[22m`

## Color Control

### Enabling/Disabling

Colors are enabled by default. They are disabled when:

1. `--no-colors` flag is passed on the command line
2. `NO_COLOR` environment variable is set to any non-empty value (follows [no-color.org](https://no-color.org/) standard)
3. `--no-expand-tokens` flag is passed (test-only alias for `--no-colors`)

When disabled, all styling functions return the input text unchanged (no ANSI codes).

### Terminal Control

The following terminal control sequences are always emitted regardless of color state:

- Alternate screen enter/exit: `\x1b[?1049h` / `\x1b[?1049l`
- Cursor hide/show: `\x1b[?25l` / `\x1b[?25h`
- Cursor blink/steady/default: `\x1b[1 q` / `\x1b[2 q` / `\x1b[0 q`
- Clear line/screen: `\x1b[K` / `\x1b[2J`
- Home cursor: `\x1b[H`
- Set window title: `\x1b]2;{title}\x07`

## Unicode Width

The `metrics` module in `src/ansi.rs` handles display width calculation:

- Variation selectors (U+FE00–U+FE0F): width 0
- Emoji (U+1F300–U+1FAFF): width 2
- Everything else (ASCII, arrows, box drawing, ellipsis): width 1

Functions:
- `metrics::visible_width(text)` — strip ANSI, then sum char widths
- `metrics::truncate(text, max_width, overflow)` — truncate to visible width, preserving ANSI codes
- `metrics::truncate_from_start(text, max_width)` — keep trailing portion, preserving leading ANSI

## Usage Patterns

### Fuzzy Match Highlighting

```
Input text: "2025-11-29-test"
Query: "te"
Rendered: dim("2025-11-29-") + highlight("te") + "st"
Displayed: [gray]"2025-11-29-"[reset] + [bold yellow]"te"[reset] + "st"
```

### Date Prefix Dimming

```
Directory: "2025-11-29-project"
Rendered: dim("2025-11-29-") + "project"
Displayed: [gray]"2025-11-29-"[reset] + "project"
```

### UI Elements

```
accent("Try Directory Selection")
dim("Search: ") + input_field.render()
```

## Design Principles

- **Direct ANSI**: No intermediate token layer — functions emit escape codes directly
- **Consistent**: Centralized function definitions in `ansi.rs` ensure uniform appearance
- **Extensible**: New styles are added as functions in `text` or `palette` modules
- **Graceful degradation**: When colors are disabled, functions return plain text
