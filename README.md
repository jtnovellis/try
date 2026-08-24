# try - fresh directories for every vibe

**[GitHub](https://github.com/jtnovellis/try)**

*Your experiments deserve a home.* 🏠

> For everyone who constantly creates new projects for little experiments, a single-file Rust CLI to quickly manage and navigate to keep them somewhat organized

Ever find yourself with 50 directories named `test`, `test2`, `new-test`, `actually-working-test`, scattered across your filesystem? Or worse, just coding in `/tmp` and losing everything?

**try** is here for your beautifully chaotic mind.

# What it does

![Fuzzy Search Demo](docs/try-fuzzy-search-demo.gif)

*[View interactive version on asciinema](https://asciinema.org/a/ve8AXBaPhkKz40YbqPTlVjqgs)*

Instantly navigate through all your experiment directories with:
- **Fuzzy search** that just works
- **Smart sorting** - recently used stuff bubbles to the top
- **Auto-dating** - creates directories like `2025-08-17-redis-experiment`
- **Zero config** - just one binary, no dependencies

## Installation

### Curl install (Recommended)

```bash
curl -sL https://raw.githubusercontent.com/jtnovellis/try/main/install.sh | sh
```

This downloads a pre-compiled binary to `~/.local/bin/try` (statically linked on Linux, verified against the release `SHA256SUMS`). Then add to your shell:

```bash
# Bash/Zsh - add to ~/.zshrc or ~/.bashrc
eval "$(~/.local/bin/try init)"

# Fish - add to ~/.config/fish/config.fish
~/.local/bin/try init | source
```

You can also specify a custom tries path and install directory:

```bash
curl -sL https://raw.githubusercontent.com/jtnovellis/try/main/install.sh | sh -s -- --path ~/my-tries --dir /usr/local/bin
```

Or pin a specific release:

```bash
curl -sL https://raw.githubusercontent.com/jtnovellis/try/main/install.sh | sh -s -- --version v1.10.1
```

Pre-built binaries are published for `x86_64`/`aarch64` on Linux and macOS. If your
platform has no binary, the script tells you and points at `make build`.

### Build from source

```bash
git clone https://github.com/jtnovellis/try.git
cd try
make build
```

This produces a static binary at `./target/release/try` (no runtime dependencies).

### Option 1: Shell integration (manual)

Add the `try` function to your shell config:

```bash
# Bash/Zsh - add to ~/.zshrc or ~/.bashrc
eval "$(./target/release/try init)"

# Fish - add to ~/.config/fish/config.fish
./target/release/try init | source
```

### Option 2: Install to ~/.local/bin

```bash
make install
```

Then add to your shell:

```bash
# Bash/Zsh
eval "$(~/.local/bin/try init ~/src/tries)"

# Fish
~/.local/bin/try init ~/src/tries | source
```

### Option 3: Auto-install shell integration

Run `try install` to automatically append the shell function to your RC file:

```bash
./target/release/try install
```

This detects your shell (bash/zsh/fish), finds the RC file, and adds the integration.

### Option 4: Nix

```bash
nix run github:jtnovellis/try
nix run github:jtnovellis/try -- --help
nix run github:jtnovellis/try init ~/my-tries
```

#### Home Manager

```nix
{
  inputs.try.url = "github:jtnovellis/try";

  imports = [ inputs.try.homeManagerModules.default ];

  programs.try = {
    enable = true;
    path = "~/experiments";  # optional, defaults to ~/src/tries
  };
}
```

## The Problem

You're learning Redis. You create `/tmp/redis-test`. Then `~/Desktop/redis-actually`. Then `~/projects/testing-redis-again`. Three weeks later you can't find that brilliant connection pooling solution you wrote at 2am.

## The Solution

All your experiments in one place, with instant fuzzy search:

```bash
$ try pool
→ 2025-08-14-redis-connection-pool    2h, 18.5
  2025-08-03-thread-pool              3d, 12.1
  2025-07-22-db-pooling               2w, 8.3
  + Create new: pool
```

Type, arrow down, enter. You're there.

## Features

### 🎯 Smart Fuzzy Search
Not just substring matching - it's smart:
- `rds` matches `redis-server`
- `connpool` matches `connection-pool`
- Recent stuff scores higher
- Shorter names win on equal matches

### ⏰ Time-Aware
- Shows how long ago you touched each project
- Recently accessed directories float to the top
- Perfect for "what was I working on yesterday?"

### 🎨 Pretty TUI
- Clean, minimal interface
- Highlights matches as you type
- Shows scores so you know why things are ranked
- Dark mode by default (because obviously)

### 📁 Organized Chaos
- Everything lives in `~/src/tries` (configurable via `TRY_PATH`)
- Auto-prefixes with dates: `2025-08-17-your-idea`
- Skip the date prompt if you already typed a name

### Shell Integration

- Bash/Zsh:

  ```bash
  # default is ~/src/tries
  eval "$(try init)"
  # or pick a path
  eval "$(try init ~/src/tries)"
  ```

- Fish:

  ```fish
  try init | source
  # or pick a path
  try init ~/src/tries | source
  ```

Notes:
- The runtime commands printed by `try` are shell-neutral (absolute paths, quoted). Only the small wrapper function differs per shell.

## Usage

```bash
try                                          # Browse all experiments
try redis                                    # Jump to redis experiment or create new
try new api                                  # Start with "2025-08-17-new-api"
try . [name]                                   # Create a dated worktree dir for current repo
try ./path/to/repo [name]                      # Use another repo as the worktree source
try worktree dir [name]                        # Same as above, explicit CLI form
try clone https://github.com/user/repo.git  # Clone repo into date-prefixed directory
try https://github.com/user/repo.git        # Shorthand for clone (same as above)
try --help                                   # See all options
```

Notes on worktrees (`try .` / `try worktree dir`):
- With a custom [name], uses that; otherwise uses cwd’s basename. Both are prefixed with today’s date.
- Inside a Git repo: adds a detached HEAD git worktree to the created directory.
- Outside a repo: simply creates the directory and changes into it.

### Git Repository Cloning

**try** can automatically clone git repositories into properly named experiment directories:

```bash
# Clone with auto-generated directory name
try clone https://github.com/jtnovellis/try.git
# Creates: 2025-08-27-jtnovellis-try

# Clone with custom name
try clone https://github.com/jtnovellis/try.git my-fork
# Creates: my-fork

# Shorthand syntax (no need to type 'clone')
try https://github.com/jtnovellis/try.git
# Creates: 2025-08-27-jtnovellis-try

# Paste a GitHub pull request URL to clone and check out that PR
try https://github.com/jtnovellis/try/pull/124
# Creates: 2025-08-27-jtnovellis-try
```

Supported git URI formats:
- `https://github.com/user/repo.git` (HTTPS GitHub)
- `git@github.com:user/repo.git` (SSH GitHub)
- `https://gitlab.com/user/repo.git` (GitLab)
- `git@host.com:user/repo.git` (SSH other hosts)
- `ssh://git@host.com:port/user/repo.git` (SSH other hosts with custom port)
- `user@host:path/to/repo.git` (nested SCP-style SSH URLs)
- `https://github.com/user/repo/pull/123` (GitHub pull requests)

A GitHub pull request URL clones the main repository, fetches the PR ref, and
checks it out in detached HEAD state. The directory name is based on the main
repository URL, not the `/pull/<number>` suffix. The `.git` suffix is
automatically removed from URLs when generating directory names.

### Keyboard Shortcuts

- `↑/↓` or `Ctrl-P/N/J/K` - Navigate
- `Enter` - Select or create
- `Ctrl-R` - Rename directory
- `Ctrl-G` - Graduate (promote try to project)
- `Ctrl-D` - Mark for deletion
- `Ctrl-T` - Create new try
- `Backspace` - Delete character
- `ESC` - Cancel
- Just type to filter

## Configuration

Set `TRY_PATH` to change where experiments are stored:

```bash
export TRY_PATH=~/code/sketches
```

Default: `~/src/tries`

## Why Rust?

- Single static binary, no runtime dependencies
- Fast and memory-safe
- Cross-platform (Linux, macOS)
- Easy to hack on

## The Philosophy

Your brain doesn't work in neat folders. You have ideas, you try things, you context-switch like a caffeinated squirrel. This tool embraces that.

Every experiment gets a home. Every home is instantly findable. Your 2am coding sessions are no longer lost to the void.

## FAQ

**Q: Why not just use `cd` and `ls`?**
A: Because you have 200 directories and can't remember if you called it `test-redis`, `redis-test`, or `new-redis-thing`.

**Q: Why not use `fzf`?**
A: fzf is great for files. This is specifically for project directories, with time-awareness and auto-creation built in.

**Q: Can I use this for real projects?**
A: You can, but it's designed for experiments. Real projects deserve real names in real locations.

**Q: What if I have thousands of experiments?**
A: First, welcome to the club. Second, it handles it fine - the scoring algorithm ensures relevant stuff stays on top.

## Contributing

It's a Rust project with zero external dependencies. Build with `make build`, test with `make test`. Send a PR if you think others would like it too.

## License

MIT - Do whatever you want with it.

---

*Built for developers with ADHD by developers with ADHD.*

*Your experiments deserve a home.* 🏠
