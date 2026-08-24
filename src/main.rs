mod ansi;
mod cli;
mod date;
mod fuzzy;
mod git_uri;
mod script;
mod selector;
mod shell;
mod term;
mod tui;

use std::path::Path;
use std::io::Write;

fn main() {
    let raw_args: Vec<String> = std::env::args().collect();
    // args[0] is program name; the rest are the real args
    let mut args: Vec<String> = raw_args[1..].to_vec();

    // Initialize color state from env
    ansi::init_colors_from_env();

    // Process color-related flags early (like the Ruby version)
    let disable_colors = take_flag(&mut args, "--no-colors") || take_flag(&mut args, "--no-expand-tokens");
    if disable_colors {
        ansi::disable_colors();
    }
    if std::env::var("NO_COLOR").map(|s| !s.is_empty()).unwrap_or(false) {
        ansi::disable_colors();
    }

    // Global help
    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        cli::print_global_help();
        std::process::exit(0);
    }

    // Version
    if args.contains(&"--version".to_string()) || args.contains(&"-v".to_string()) {
        eprintln!("try {}", cli::VERSION);
        std::process::exit(0);
    }

    // Extract all options before getting command
    let tries_path = extract_option_with_value(&mut args, "--path")
        .map(|p| expand_path(&p))
        .unwrap_or_else(|| selector::default_try_path());
    let tries_path = expand_path(&tries_path);

    // Test-only flags
    let and_type = extract_option_with_value(&mut args, "--and-type");
    let and_exit = take_flag(&mut args, "--and-exit");
    let and_keys_raw = extract_option_with_value(&mut args, "--and-keys");
    let and_confirm = extract_option_with_value(&mut args, "--and-confirm");

    let command = args.first().cloned();

    let result = match command.as_deref() {
        None => {
            cli::print_global_help();
            std::process::exit(2);
        }
        Some("clone") => {
            let script = cmd_clone(&mut args, &tries_path);
            script::emit_script(&script);
            std::process::exit(0);
        }
        Some("init") => {
            cmd_init(&mut args, &tries_path);
            std::process::exit(0);
        }
        Some("install") => {
            cmd_install(&mut args, &tries_path);
        }
        Some("exec") => {
            let sub = args.get(1).cloned();
            match sub.as_deref() {
                Some("clone") => {
                    args.remove(1); // remove "clone"
                    let script = cmd_clone(&mut args, &tries_path);
                    script::emit_script(&script);
                    std::process::exit(0);
                }
                Some("worktree") => {
                    args.remove(1); // remove "worktree"
                    let repo = args.get(1).cloned();
                    let repo_dir = if let Some(r) = repo.as_deref() {
                        if r != "dir" {
                            expand_path(r)
                        } else {
                            current_dir()
                        }
                    } else {
                        current_dir()
                    };
                    let custom = args[2..].join(" ");
                    let full_path = script::worktree_path(&tries_path, &repo_dir, Some(&custom));
                    let script = script::script_worktree(
                        &full_path,
                        if repo_dir == current_dir() { None } else { Some(&repo_dir) },
                    );
                    script::emit_script(&script);
                    std::process::exit(0);
                }
                Some("cd") => {
                    args.remove(1); // remove "cd"
                    let script = cmd_cd(&mut args, &tries_path, &and_type, and_exit, &and_keys_raw, &and_confirm);
                    if let Some(s) = script {
                        script::emit_script(&s);
                        std::process::exit(0);
                    } else {
                        println!("Cancelled.");
                        std::process::exit(1);
                    }
                }
                _ => {
                    // Default: try exec [query]
                    let script = cmd_cd(&mut args, &tries_path, &and_type, and_exit, &and_keys_raw, &and_confirm);
                    if let Some(s) = script {
                        script::emit_script(&s);
                        std::process::exit(0);
                    } else {
                        println!("Cancelled.");
                        std::process::exit(1);
                    }
                }
            }
        }
        Some("worktree") => {
            let repo = args.get(1).cloned();
            let repo_dir = if let Some(r) = repo.as_deref() {
                if r != "dir" {
                    expand_path(r)
                } else {
                    current_dir()
                }
            } else {
                current_dir()
            };
            let custom = args[2..].join(" ");
            let full_path = script::worktree_path(&tries_path, &repo_dir, Some(&custom));
            let script = script::script_worktree(
                &full_path,
                if repo_dir == current_dir() { None } else { Some(&repo_dir) },
            );
            script::emit_script(&script);
            std::process::exit(0);
        }
        Some(cmd) => {
            // Default: try [query] - same as try exec [query]
            // Prepend the command back as a search query
            let mut query_args = vec![cmd.to_string()];
            query_args.extend(args.into_iter().skip(1));
            let script = cmd_cd(&mut query_args, &tries_path, &and_type, and_exit, &and_keys_raw, &and_confirm);
            if let Some(s) = script {
                script::emit_script(&s);
                std::process::exit(0);
            } else {
                println!("Cancelled.");
                std::process::exit(1);
            }
        }
    };

    let _ = result;
}

/// Remove a boolean flag from args (no value). Returns true if present.
fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(pos) = args.iter().position(|a| a == flag) {
        args.remove(pos);
        true
    } else {
        false
    }
}

/// Extract a "--name VALUE" or "--name=VALUE" option (last one wins).
fn extract_option_with_value(args: &mut Vec<String>, opt_name: &str) -> Option<String> {
    let mut found = None;
    for i in (0..args.len()).rev() {
        let a = &args[i];
        if a == opt_name || a.starts_with(&format!("{}=", opt_name)) {
            found = Some(i);
            break;
        }
    }
    let found = found?;
    let arg = args.remove(found);
    if arg.contains('=') {
        Some(arg.splitn(2, '=').nth(1).unwrap().to_string())
    } else {
        Some(args.remove(found))
    }
}

/// Expand a path like Ruby's File.expand_path:
/// - Expand ~ to HOME
/// - Make relative paths absolute (join with cwd)
/// - NEVER resolve symlinks (unlike std::fs::canonicalize)
/// - Normalize . and .. components
fn expand_path(path: &str) -> String {
    let expanded = shell::expand_tilde(path);

    // If absolute, normalize it; if relative, join with cwd
    let absolute = if Path::new(&expanded).is_absolute() {
        expanded.clone()
    } else {
        format!("{}/{}", current_dir(), expanded)
    };

    // Normalize . and .. components without resolving symlinks
    normalize_path(&absolute)
}

/// Normalize a path by resolving . and .. lexically (no symlink resolution).
fn normalize_path(path: &str) -> String {
    let mut components: Vec<String> = Vec::new();
    for component in Path::new(path).components() {
        use std::path::Component;
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Only pop if the last component is a normal name (not root)
                if let Some(last) = components.last() {
                    if last != "/" && last != "//" && !last.is_empty() {
                        components.pop();
                    }
                }
            }
            Component::Normal(name) => {
                components.push(name.to_string_lossy().to_string());
            }
            Component::RootDir => {
                components.push("/".to_string());
            }
            Component::Prefix(prefix) => {
                components.push(prefix.as_os_str().to_string_lossy().to_string());
            }
        }
    }

    // Reassemble
    if components.is_empty() {
        return "/".to_string();
    }
    let mut result = String::new();
    for (i, comp) in components.iter().enumerate() {
        if comp == "/" {
            result.push('/');
        } else if i == 0 && !comp.is_empty() {
            // First component is not a root (relative path that became absolute somehow)
            result.push_str(comp);
        } else {
            if !result.ends_with('/') {
                result.push('/');
            }
            result.push_str(comp);
        }
    }
    result
}

fn current_dir() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string())
}

fn cmd_clone(args: &mut Vec<String>, tries_path: &str) -> Vec<String> {
    let git_uri = args.get(1).cloned();
    let custom_name = args.get(2).cloned();

    let git_uri = match git_uri {
        Some(u) => u,
        None => {
            eprintln!("Error: git URI required for clone command");
            eprintln!("Usage: try clone <git-uri> [name]");
            std::process::exit(1);
        }
    };

    let dir_name = git_uri::generate_clone_directory_name(
        &git_uri,
        custom_name.as_deref(),
    );
    let dir_name = match dir_name {
        Some(n) => n,
        None => {
            eprintln!("Error: Unable to parse git URI: {}", git_uri);
            std::process::exit(1);
        }
    };

    let path = Path::new(tries_path).join(&dir_name);
    let path_str = path.to_string_lossy().to_string();

    if let Some(pr) = git_uri::github_pr_details(&git_uri) {
        script::script_clone_pr(&path_str, &pr.git_uri, &pr.pr_id)
    } else {
        script::script_clone(&path_str, &git_uri)
    }
}

fn cmd_init(args: &mut Vec<String>, _tries_path: &str) {
    let script_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| std::env::args().next().unwrap_or_default());

    // Determine explicit path
    let explicit_path = args.get(1).and_then(|a| {
        if a.starts_with('/') {
            Some(expand_path(a))
        } else {
            None
        }
    });

    let default_path = shell::expand_tilde("~/src/tries");
    let shell_type = if shell::is_fish() { "fish" } else { "bash" };
    let snippet = shell::init_snippet(
        shell_type,
        &script_path,
        explicit_path.as_deref(),
        &default_path,
    );
    println!("{}", snippet);
}

fn cmd_install(args: &mut Vec<String>, tries_path: &str) {
    let script_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| std::env::args().next().unwrap_or_default());

    let explicit_path = args.get(1).and_then(|a| {
        if a.starts_with('/') {
            Some(expand_path(a))
        } else {
            None
        }
    });

    let default_path = shell::expand_tilde("~/src/tries");
    let shell_type = shell::detect_shell().unwrap_or_else(|| "bash".to_string());
    let rc_file = shell::shell_rc_file(&shell_type);

    let rc_file = match rc_file {
        Some(f) => f,
        None => {
            eprintln!("Error: could not determine shell config file");
            eprintln!("Your shell was detected as: {}", shell_type);
            eprintln!("Run 'try init' and manually add the output to your shell config.");
            std::process::exit(1);
        }
    };

    let snippet = shell::init_snippet(
        &shell_type,
        &script_path,
        explicit_path.as_deref(),
        &default_path,
    );

    let rc_path = shell::expand_tilde(&rc_file);

    if Path::new(&rc_path).exists() {
        if let Ok(content) = std::fs::read_to_string(&rc_path) {
            if content.contains("# try shell integration") {
                eprintln!("try is already installed in {}", rc_path);
                eprintln!("To reinstall, remove the '# try shell integration' block first.");
                std::process::exit(0);
            }
        }
    }

    let block = format!("\n# try shell integration\n{}", snippet);

    if Path::new(&rc_path).exists() {
        let metadata = std::fs::metadata(&rc_path);
        if let Ok(m) = metadata {
            if m.permissions().readonly() {
                eprintln!("Warning: {} is read-only, skipping.", rc_path);
                eprintln!("Run 'try init' and manually add the output to your shell config.");
                std::process::exit(1);
            }
        }
    }

    if let Some(parent) = Path::new(&rc_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rc_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error writing to {}: {}", rc_path, e);
            std::process::exit(1);
        }
    };
    let _ = file.write_all(block.as_bytes());
    eprintln!("Added try shell integration to {}", rc_path);
    if shell_type != "pwsh" {
        eprintln!("Restart your shell or run: source {}", rc_path);
    } else {
        eprintln!("Restart your shell or run: . $PROFILE");
    }
    std::process::exit(0);
}

fn cmd_cd(
    args: &mut Vec<String>,
    tries_path: &str,
    and_type: &Option<String>,
    and_exit: bool,
    and_keys_raw: &Option<String>,
    and_confirm: &Option<String>,
) -> Option<Vec<String>> {
    // Support: try . [name] and try ./path [name]
    if let Some(first) = args.get(1).cloned() {
        if first == "clone" {
            let rest: Vec<String> = args[2..].to_vec();
            return Some(cmd_clone(&mut rest.clone(), tries_path));
        }

        if first.starts_with('.') {
            let path_arg = args.remove(1);
            let custom: String = args[1..].join(" ");
            let repo_dir = expand_path(&path_arg);
            if path_arg == "." && custom.trim().is_empty() {
                eprintln!("Error: 'try .' requires a name argument");
                eprintln!("Usage: try . <name>");
                std::process::exit(1);
            }
            let base = if !custom.trim().is_empty() {
                custom.replace(char::is_whitespace, "-")
            } else {
                Path::new(&repo_dir)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            };
            let date_prefix = date::today_date_prefix();
            let base = script::resolve_unique_name_with_versioning(tries_path, &date_prefix, &base);
            let full_path = Path::new(tries_path).join(format!("{}-{}", date_prefix, base));
            let full_path_str = full_path.to_string_lossy().to_string();

            let git_path = Path::new(&repo_dir).join(".git");
            if git_path.exists() {
                return Some(script::script_worktree(&full_path_str, Some(&repo_dir)));
            } else {
                return Some(script::script_mkdir_cd(&full_path_str));
            }
        }
    }

    let search_term: String = args[1..].join(" ");

    // Git URL shorthand → clone workflow
    let first_word = search_term.split_whitespace().next().unwrap_or("");
    if git_uri::is_git_uri(first_word) {
        let parts: Vec<&str> = search_term.splitn(2, char::is_whitespace).collect();
        let git_uri = parts[0].to_string();
        let custom_name = parts.get(1).map(|s| s.to_string());
        let dir_name = git_uri::generate_clone_directory_name(&git_uri, custom_name.as_deref());
        let dir_name = match dir_name {
            Some(n) => n,
            None => {
                eprintln!("Error: Unable to parse git URI: {}", git_uri);
                std::process::exit(1);
            }
        };
        let full_path = Path::new(tries_path).join(&dir_name);
        let full_path_str = full_path.to_string_lossy().to_string();
        if let Some(pr) = git_uri::github_pr_details(&git_uri) {
            return Some(script::script_clone_pr(&full_path_str, &pr.git_uri, &pr.pr_id));
        } else {
            return Some(script::script_clone(&full_path_str, &git_uri));
        }
    }

    // Regular interactive selector
    let test_keys = parse_test_keys(and_keys_raw);
    let mut selector = selector::TrySelector::new(
        &search_term,
        tries_path,
        and_type.as_deref(),
        and_exit,
        and_exit || (test_keys.is_some() && !test_keys.as_ref().unwrap().is_empty()),
        test_keys.unwrap_or_default(),
        and_confirm.clone(),
    );

    let result = selector.run();
    let result = result?;

    let script = match result {
        selector::Selection::Cd { path } => script::script_cd(&path),
        selector::Selection::Mkdir { path } => script::script_mkdir_cd(&path),
        selector::Selection::Delete { paths, base_path } => script::script_delete(&paths, &base_path),
        selector::Selection::Rename { old, new, base_path } => {
            script::script_rename(&base_path, &old, &new)
        }
        selector::Selection::Ascend { source, dest, basename, base_path } => {
            script::script_ascend(&source, &dest, &basename, &base_path)
        }
        selector::Selection::Cancel => return None,
    };

    Some(script)
}

/// Parse the --and-keys spec into a list of key strings.
fn parse_test_keys(spec: &Option<String>) -> Option<Vec<String>> {
    let spec = spec.as_ref()?;
    if spec.is_empty() {
        return None;
    }

    // Detect mode: if contains comma OR is purely uppercase letters/hyphens, use token mode
    let use_token_mode = spec.contains(',') || spec.chars().all(|c| c.is_ascii_uppercase() || c == '-');

    if use_token_mode {
        let tokens: Vec<&str> = spec.split(',').map(|s| s.trim()).collect();
        let mut keys = Vec::new();
        for tok in tokens {
            let up = tok.to_uppercase();
            match up.as_str() {
                "UP" => keys.push("\x1b[A".to_string()),
                "DOWN" => keys.push("\x1b[B".to_string()),
                "LEFT" => keys.push("\x1b[D".to_string()),
                "RIGHT" => keys.push("\x1b[C".to_string()),
                "ENTER" => keys.push("\r".to_string()),
                "ESC" => keys.push("\x1b".to_string()),
                "BACKSPACE" => keys.push("\x7f".to_string()),
                "CTRL-A" | "CTRLA" => keys.push("\x01".to_string()),
                "CTRL-B" | "CTRLB" => keys.push("\x02".to_string()),
                "CTRL-D" | "CTRLD" => keys.push("\x04".to_string()),
                "CTRL-E" | "CTRLE" => keys.push("\x05".to_string()),
                "CTRL-F" | "CTRLF" => keys.push("\x06".to_string()),
                "CTRL-G" | "CTRLG" => keys.push("\x07".to_string()),
                "CTRL-H" | "CTRLH" => keys.push("\x08".to_string()),
                "CTRL-J" | "CTRLJ" => keys.push("\x0a".to_string()),
                "CTRL-K" | "CTRLK" => keys.push("\x0b".to_string()),
                "CTRL-N" | "CTRLN" => keys.push("\x0e".to_string()),
                "CTRL-P" | "CTRLP" => keys.push("\x10".to_string()),
                "CTRL-R" | "CTRLR" => keys.push("\x12".to_string()),
                "CTRL-T" | "CTRLT" => keys.push("\x14".to_string()),
                "CTRL-U" | "CTRLU" => keys.push("\x15".to_string()),
                "CTRL-W" | "CTRLW" => keys.push("\x17".to_string()),
                "DELETE" => keys.push("\x1b[3~".to_string()),
                _ => {
                    if let Some(text) = tok.strip_prefix("TYPE=") {
                        for ch in text.chars() {
                            keys.push(ch.to_string());
                        }
                    } else if tok.len() == 1 {
                        keys.push(tok.to_string());
                    }
                }
            }
        }
        Some(keys)
    } else {
        // Raw character mode
        let mut keys = Vec::new();
        let bytes = spec.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == 0x1b && i + 2 < bytes.len() && bytes[i + 1] == b'[' {
                keys.push(String::from_utf8_lossy(&bytes[i..i + 3]).to_string());
                i += 3;
            } else {
                keys.push((bytes[i] as char).to_string());
                i += 1;
            }
        }
        Some(keys)
    }
}
