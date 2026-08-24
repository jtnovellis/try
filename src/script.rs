use std::path::Path;

pub const SCRIPT_WARNING: &str =
    "# if you can read this, you didn't launch try from an alias. run try --help.";

/// Shell-quote a single-quoted string (Ruby's q() equivalent).
pub fn q(s: &str) -> String {
    format!(
        "'{}'",
        s.replace('\'', "'\"'\"'")
    )
}

/// Emit a shell script: warning comment + commands chained with `&& \`.
pub fn emit_script(cmds: &[String]) {
    println!("{}", SCRIPT_WARNING);
    for (i, cmd) in cmds.iter().enumerate() {
        if i == 0 {
            print!("{}", cmd);
        } else {
            print!("  {}", cmd);
        }
        if i < cmds.len() - 1 {
            println!(" && \\");
        } else {
            println!();
        }
    }
}

/// cd + touch + echo + optional terminal rename commands.
pub fn script_cd(path: &str) -> Vec<String> {
    let mut cmds = vec![
        format!("touch {}", q(path)),
        format!("echo {}", q(path)),
        format!("cd {}", q(path)),
    ];
    cmds.extend(terminal_rename_commands(path));
    cmds
}

pub fn script_mkdir_cd(path: &str) -> Vec<String> {
    let mut cmds = vec![format!("mkdir -p {}", q(path))];
    cmds.extend(script_cd(path));
    cmds
}

pub fn script_clone(path: &str, uri: &str) -> Vec<String> {
    let mut cmds = vec![
        format!("mkdir -p {}", q(path)),
        format!("echo {}", q(&format!("Using git clone to create this trial from {}.", uri))),
        format!("git clone '{}' {}", uri, q(path)),
    ];
    cmds.extend(script_cd(path));
    cmds
}

pub fn script_clone_pr(path: &str, uri: &str, pr_id: &str) -> Vec<String> {
    let ref_str = format!("pull/{}/head", pr_id);
    let mut cmds = vec![
        format!("mkdir -p {}", q(path)),
        format!(
            "echo {}",
            q(&format!("Using git clone to create this trial from {} PR #{}.", uri, pr_id))
        ),
        format!("git clone {} {}", q(uri), q(path)),
        format!("git -C {} fetch origin {}", q(path), q(&ref_str)),
        format!("git -C {} checkout --detach FETCH_HEAD", q(path)),
    ];
    cmds.extend(script_cd(path));
    cmds
}

pub fn script_worktree(path: &str, repo: Option<&str>) -> Vec<String> {
    let worktree_cmd = if let Some(r) = repo {
        format!(
            "/usr/bin/env sh -c 'if git -C {r} rev-parse --is-inside-work-tree >/dev/null 2>&1; then repo=$(git -C {r} rev-parse --show-toplevel); git -C \"$repo\" worktree add --detach {p} >/dev/null 2>&1 || true; fi; exit 0'",
            r = q(r),
            p = q(path)
        )
    } else {
        format!(
            "/usr/bin/env sh -c 'if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then repo=$(git rev-parse --show-toplevel); git -C \"$repo\" worktree add --detach {p} >/dev/null 2>&1 || true; fi; exit 0'",
            p = q(path)
        )
    };
    let src = repo.map(|r| r.to_string()).unwrap_or_else(|| std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default());
    let mut cmds = vec![
        format!("mkdir -p {}", q(path)),
        format!("echo {}", q(&format!("Using git worktree to create this trial from {}.", src))),
        worktree_cmd,
    ];
    cmds.extend(script_cd(path));
    cmds
}

pub fn script_delete(paths: &[(String, String)], base_path: &str) -> Vec<String> {
    // paths: Vec of (real_path, basename)
    let mut cmds = vec![format!("cd {}", q(base_path))];
    for (real_path, basename) in paths {
        // The Ruby version uses basename for the rm command
        let _ = real_path;
        cmds.push(format!("test -d {} && rm -rf {}", q(basename), q(basename)));
    }
    let pwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    cmds.push(format!("cd {} 2>/dev/null || cd {}", q(&pwd), q(base_path)));
    cmds
}

pub fn script_rename(base_path: &str, old_name: &str, new_name: &str) -> Vec<String> {
    let new_path = Path::new(base_path).join(new_name);
    let new_path = new_path.to_string_lossy().to_string();
    let mut cmds = vec![
        format!("cd {}", q(base_path)),
        format!("mv {} {}", q(old_name), q(new_name)),
        format!("echo {}", q(&new_path)),
        format!("cd {}", q(&new_path)),
    ];
    cmds.extend(terminal_rename_commands(&new_path));
    cmds
}

pub fn script_ascend(
    source: &str,
    dest: &str,
    basename: &str,
    base_path: &str,
) -> Vec<String> {
    let symlink_path = Path::new(base_path).join(basename);
    let symlink_path = symlink_path.to_string_lossy().to_string();

    // Check if source is a git worktree (has .git file, not directory)
    let git_file = Path::new(source).join(".git");
    let is_worktree = git_file.is_file();

    let mut cmds = Vec::new();
    if is_worktree {
        cmds.push(format!("git worktree move {} {}", q(source), q(dest)));
    } else {
        cmds.push(format!("mv {} {}", q(source), q(dest)));
    }
    cmds.push(format!("ln -s {} {}", q(dest), q(&symlink_path)));
    cmds.push(format!(
        "echo {}",
        q(&format!("Graduated: {} → {}", basename, dest))
    ));
    cmds.extend(script_cd(dest));
    cmds
}

/// Emit best-effort terminal manager rename commands.
fn terminal_rename_commands(path: &str) -> Vec<String> {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    // Strip date prefix
    let name = if name.len() > 11 {
        let prefix = &name[..11]; // YYYY-MM-DD-
        if prefix.chars().nth(4) == Some('-')
            && prefix.chars().nth(7) == Some('-')
            && prefix.chars().nth(10) == Some('-')
        {
            &name[11..]
        } else {
            &name
        }
    } else {
        &name
    };
    let label = format!("try: {}", name);

    let herdr_env = std::env::var("HERDR_ENV").unwrap_or_default();
    let herdr_pane = std::env::var("HERDR_PANE_ID").unwrap_or_default();
    let herdr_workspace = std::env::var("HERDR_WORKSPACE_ID").unwrap_or_default();

    if herdr_env == "1" && !herdr_pane.is_empty() {
        let mut commands = vec![
            format!(
                "command -v herdr >/dev/null 2>&1 && herdr pane report-metadata {} --source try --title {} >/dev/null 2>&1 || true",
                q(&herdr_pane),
                q(&label)
            ),
        ];
        if herdr_pane.ends_with(":p1") && !herdr_workspace.is_empty() {
            commands.push(format!(
                "command -v herdr >/dev/null 2>&1 && herdr workspace rename {} {} >/dev/null 2>&1 || true",
                q(&herdr_workspace),
                q(&label)
            ));
        }
        commands
    } else {
        let cmux_socket = std::env::var("CMUX_SOCKET_PATH").unwrap_or_default();
        let cmux_bundle = std::env::var("CMUX_BUNDLE_ID").unwrap_or_default();
        if !cmux_socket.is_empty() || !cmux_bundle.is_empty() {
            vec![format!(
                "command -v cmux >/dev/null 2>&1 && cmux rename-tab {} >/dev/null 2>&1 || true",
                q(&label)
            )]
        } else {
            vec![]
        }
    }
}

/// Resolve a unique directory name under tries_path by appending -2, -3, ...
pub fn unique_dir_name(tries_path: &str, dir_name: &str) -> String {
    let mut candidate = dir_name.to_string();
    let mut i = 2;
    loop {
        let path = Path::new(tries_path).join(&candidate);
        if !path.exists() {
            return candidate;
        }
        candidate = format!("{}-{}", dir_name, i);
        i += 1;
    }
}

/// If the given base ends with digits and today's dir already exists,
/// bump the trailing number. Otherwise fall back to unique_dir_name.
pub fn resolve_unique_name_with_versioning(
    tries_path: &str,
    date_prefix: &str,
    base: &str,
) -> String {
    let initial = format!("{}-{}", date_prefix, base);
    let initial_path = Path::new(tries_path).join(&initial);
    if !initial_path.exists() {
        return base.to_string();
    }

    // Check if base ends with digits: ^(.*?)(\d+)$
    // We find the last contiguous run of digits
    let digit_start = base.rfind(|c: char| !c.is_ascii_digit());
    let (stem, num_str) = if let Some(pos) = digit_start {
        let digits = &base[pos + 1..];
        if digits.chars().all(|c| c.is_ascii_digit()) && !digits.is_empty() {
            (&base[..=pos], digits)
        } else {
            // No numeric suffix
            let full = unique_dir_name(tries_path, &initial);
            // Strip the date prefix
            let prefix = format!("{}-", date_prefix);
            return full.strip_prefix(&prefix).unwrap_or(&full).to_string();
        }
    } else {
        // All digits
        ("", base)
    };

    let mut n: u64 = num_str.parse().unwrap_or(0);
    loop {
        n += 1;
        let candidate_base = format!("{}{}", stem, n);
        let candidate_full = Path::new(tries_path).join(format!("{}-{}", date_prefix, candidate_base));
        if !candidate_full.exists() {
            return candidate_base;
        }
    }
}

pub fn worktree_path(tries_path: &str, repo_dir: &str, custom_name: Option<&str>) -> String {
    let base = if let Some(name) = custom_name {
        let name = name.trim();
        if !name.is_empty() {
            name.replace(char::is_whitespace, "-")
        } else {
            // Use basename of realpath
            let real = std::fs::canonicalize(repo_dir)
                .unwrap_or_else(|_| std::path::PathBuf::from(repo_dir));
            real.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        }
    } else {
        let real = std::fs::canonicalize(repo_dir)
            .unwrap_or_else(|_| std::path::PathBuf::from(repo_dir));
        real.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    };

    let date_prefix = crate::date::today_date_prefix();
    let base = resolve_unique_name_with_versioning(tries_path, &date_prefix, &base);
    Path::new(tries_path)
        .join(format!("{}-{}", date_prefix, base))
        .to_string_lossy()
        .to_string()
}
