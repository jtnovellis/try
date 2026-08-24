pub const VERSION: &str = "1.10.1";

pub fn print_global_help() {
    eprint!(
        r#"try v{} - ephemeral workspace manager

To use try, add to your shell config:

  # bash/zsh (~/.bashrc or ~/.zshrc)
  eval "$(try init ~/src/tries)"

  # fish (~/.config/fish/config.fish)
  eval (try init ~/src/tries | string collect)

Usage:
  try [query]           Interactive directory selector
  try clone <url>       Clone repo into dated directory
  try worktree <name>   Create worktree from current git repo
  try . <name>          Shorthand for worktree (uses cwd basename)
  try install           Auto-install shell integration to RC file
  try --help            Show this help

Commands:
  init [path]           Output shell function definition
  install [path]         Append shell function to RC file
  clone <url> [name]    Clone git repo into date-prefixed directory
  worktree <name>       Create worktree in dated directory

Examples:
  try                   Open interactive selector
  try project           Selector with initial filter
  try clone https://github.com/user/repo
  try https://github.com/user/repo/pull/123
  try worktree feature-branch

Manual mode (without alias):
  try exec [query]      Output shell script to eval

Flags:
  --path <dir>          Override tries directory for this call
  --no-colors           Disable ANSI color codes
  --help, -h            Show this help
  --version, -v          Show version number

Environment:
  TRY_PATH          Tries directory (default: ~/src/tries)
  TRY_PROJECTS      Graduate destination (default: parent of TRY_PATH)

Keyboard:
  ↑/↓, Ctrl-P/N     Navigate
  Enter              Select / Create new
  Ctrl-R             Rename
  Ctrl-G             Graduate (promote try to project)
  Ctrl-D             Mark for deletion
  Ctrl-T             Create new try
  Esc                Cancel
"#,
        VERSION
    );
}
