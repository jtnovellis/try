use std::path::Path;

/// Detect the current shell.
pub fn detect_shell() -> Option<String> {
    let shell_env = std::env::var("SHELL").unwrap_or_default();
    if shell_env.contains("fish") {
        return Some("fish".to_string());
    }
    if shell_env.contains("zsh") {
        return Some("zsh".to_string());
    }
    if shell_env.contains("bash") {
        return Some("bash".to_string());
    }

    // PowerShell detection
    let psmodule = std::env::var("PSModulePath").unwrap_or_default();
    if !psmodule.is_empty() {
        return Some("pwsh".to_string());
    }

    // Fallback: check parent process name
    let ppid = std::process::id();
    // Best-effort: parse parent via /proc or ps
    let parent = parent_process_name(ppid).unwrap_or_default();
    if parent.contains("fish") {
        return Some("fish".to_string());
    }
    if parent.contains("zsh") {
        return Some("zsh".to_string());
    }
    if parent.contains("bash") {
        return Some("bash".to_string());
    }
    if parent.to_lowercase().contains("pwsh") || parent.to_lowercase().contains("powershell") {
        return Some("pwsh".to_string());
    }

    None
}

#[cfg(unix)]
fn parent_process_name(_pid: u32) -> Option<String> {
    // Use ps to get the parent's name
    let ppid = getppid();
    let output = std::process::Command::new("ps")
        .args(["c", "-p", &ppid.to_string(), "-o", "ucomm="])
        .output()
        .ok()?;
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

#[cfg(not(unix))]
fn parent_process_name(_pid: u32) -> Option<String> {
    None
}

#[cfg(unix)]
fn getppid() -> u32 {
    extern "C" {
        fn getppid() -> u32;
    }
    unsafe { getppid() }
}

/// Detect if the current shell is fish (for init command).
pub fn is_fish() -> bool {
    let shell = std::env::var("SHELL").unwrap_or_default();
    if !shell.is_empty() {
        return shell.contains("fish");
    }
    let ppid = std::process::id();
    let parent = parent_process_name(ppid).unwrap_or_default();
    parent.contains("fish")
}

pub fn shell_rc_file(shell: &str) -> Option<String> {
    match shell {
        "fish" => Some("~/.config/fish/config.fish".to_string()),
        "zsh" => Some("~/.zshrc".to_string()),
        "bash" => {
            // Prefer .bashrc, fall back to .bash_profile on macOS
            let bashrc = expand_tilde("~/.bashrc");
            if Path::new(&bashrc).exists() {
                Some("~/.bashrc".to_string())
            } else {
                Some("~/.bash_profile".to_string())
            }
        }
        "pwsh" => {
            let profile = std::env::var("PROFILE").ok();
            if profile.is_some() {
                profile
            } else {
                let home = std::env::var("USERPROFILE")
                    .or_else(|_| std::env::var("HOME"))
                    .unwrap_or_default();
                // Simplified — not a primary target platform
                Some(format!(
                    "{}/Documents/PowerShell/Microsoft.PowerShell_profile.ps1",
                    home
                ))
            }
        }
        _ => None,
    }
}

/// Generate the init snippet for a given shell.
pub fn init_snippet(
    shell: &str,
    script_path: &str,
    explicit_path: Option<&str>,
    default_path: &str,
) -> String {
    match shell {
        "fish" => {
            let path_arg = if let Some(p) = explicit_path {
                format!(" --path '{}'", p)
            } else {
                format!(
                    " --path (if set -q TRY_PATH; echo \"$TRY_PATH\"; else; echo '{}'; end)",
                    default_path
                )
            };
            format!(
                r#"function try
  set -l out ({prefix} exec{path_arg} $argv 2>/dev/tty | string collect)
  if test $pipestatus[1] -eq 0
    eval $out
  else
    echo $out
  end
end
"#,
                prefix = self_exec_prefix(script_path),
                path_arg = path_arg
            )
        }
        "pwsh" => {
            let path_expr = if let Some(p) = explicit_path {
                format!("'{}'", p)
            } else {
                format!(
                    "$(if ($env:TRY_PATH) {{ $env:TRY_PATH }} else {{ '{}' }})",
                    default_path
                )
            };
            format!(
                r#"function try {{
  $tryPath = {path_expr}
  $tempErr = [System.IO.Path]::GetTempFileName()
  $out = & {exec} exec --path $tryPath @args 2>$tempErr
  if ($LASTEXITCODE -eq 0) {{
    $out | Invoke-Expression
  }} else {{
    Get-Content $tempErr | Write-Host
    $out | Write-Output
  }}
  Remove-Item $tempErr -ErrorAction SilentlyContinue
}}
"#,
                path_expr = path_expr,
                exec = if compiled_binary(script_path) {
                    crate::script::q(script_path)
                } else {
                    format!("ruby '{}'", script_path)
                }
            )
        }
        _ => {
            // bash, zsh
            let path_arg = if let Some(p) = explicit_path {
                format!(" --path '{}'", p)
            } else {
                format!(" --path \"${{TRY_PATH:-{}}}\"", default_path)
            };
            format!(
                r#"try() {{
  local out
  out=$({prefix} exec{path_arg} "$@" 2>/dev/tty)
  if [ $? -eq 0 ]; then
    eval "$out"
  else
    echo "$out"
  fi
}}
"#,
                prefix = self_exec_prefix(script_path),
                path_arg = path_arg
            )
        }
    }
}

/// Whether the current binary is compiled (not a .rb file).
pub fn compiled_binary(script_path: &str) -> bool {
    !script_path.ends_with(".rb")
}

/// The exec prefix for the init snippet.
pub fn self_exec_prefix(script_path: &str) -> String {
    if compiled_binary(script_path) {
        crate::script::q(script_path)
    } else {
        format!("/usr/bin/env ruby {}", crate::script::q(script_path))
    }
}

/// Expand ~ to the home directory.
pub fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~") {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{}{}", home, rest)
    } else {
        path.to_string()
    }
}
