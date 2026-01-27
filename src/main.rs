use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::process::{exit, Command};

/// Fast extraction of a script value from package.json without full JSON parsing.
/// Returns the script command if found.
fn extract_script<'a>(content: &'a str, name: &str) -> Option<&'a str> {
    // Find "scripts" section
    let scripts_start = content.find("\"scripts\"")?;
    let after_scripts = &content[scripts_start..];

    // Find the opening brace of scripts object
    let brace_pos = after_scripts.find('{')?;
    let scripts_content = &after_scripts[brace_pos..];

    // Find closing brace (simple: find matching brace)
    let mut depth = 0;
    let mut scripts_end = 0;
    for (i, c) in scripts_content.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    scripts_end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    let scripts_block = &scripts_content[..=scripts_end];

    // Find the script name
    let search_key = format!("\"{}\"", name);
    let key_pos = scripts_block.find(&search_key)?;
    let after_key = &scripts_block[key_pos + search_key.len()..];

    // Skip to colon and whitespace
    let colon_pos = after_key.find(':')?;
    let after_colon = &after_key[colon_pos + 1..].trim_start();

    // Extract the value (must start with quote)
    if !after_colon.starts_with('"') {
        return None;
    }

    // Find the closing quote (handle escaped quotes)
    let value_start = 1; // skip opening quote
    let value_content = &after_colon[value_start..];
    let mut end = 0;
    let mut escape = false;
    for (i, c) in value_content.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' {
            escape = true;
            continue;
        }
        if c == '"' {
            end = i;
            break;
        }
    }

    Some(&value_content[..end])
}

/// Extract all script names for listing
fn extract_script_names(content: &str) -> Vec<(&str, &str)> {
    let mut results = Vec::new();

    let Some(scripts_start) = content.find("\"scripts\"") else {
        return results;
    };
    let after_scripts = &content[scripts_start..];

    let Some(brace_pos) = after_scripts.find('{') else {
        return results;
    };
    let scripts_content = &after_scripts[brace_pos..];

    // Find closing brace
    let mut depth = 0;
    let mut scripts_end = 0;
    for (i, c) in scripts_content.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    scripts_end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    let scripts_block = &scripts_content[1..scripts_end]; // skip opening brace

    // Parse each key-value pair
    let mut pos = 0;
    while pos < scripts_block.len() {
        // Find opening quote of key
        let Some(key_start) = scripts_block[pos..].find('"') else {
            break;
        };
        let key_start = pos + key_start + 1;

        // Find closing quote of key
        let Some(key_end) = scripts_block[key_start..].find('"') else {
            break;
        };
        let key_end = key_start + key_end;
        let key = &scripts_block[key_start..key_end];

        // Find colon and opening quote of value
        let after_key = &scripts_block[key_end + 1..];
        let Some(colon) = after_key.find(':') else {
            break;
        };
        let after_colon = &after_key[colon + 1..];
        let Some(val_start) = after_colon.find('"') else {
            break;
        };
        let val_start_abs = key_end + 1 + colon + 1 + val_start + 1;

        // Find closing quote of value (handle escapes)
        let val_content = &scripts_block[val_start_abs..];
        let mut val_end = 0;
        let mut escape = false;
        for (i, c) in val_content.char_indices() {
            if escape {
                escape = false;
                continue;
            }
            if c == '\\' {
                escape = true;
                continue;
            }
            if c == '"' {
                val_end = i;
                break;
            }
        }
        let value = &val_content[..val_end];

        results.push((key, value));
        pos = val_start_abs + val_end + 1;
    }

    results.sort_by_key(|(k, _)| *k);
    results
}

/// Find and read package.json in one operation - avoids separate exists() check
fn find_and_read_package_json() -> Option<(std::path::PathBuf, String)> {
    let mut dir = env::current_dir().ok()?;
    // Pre-allocate buffer for typical package.json (most are <32KB)
    let mut buf = String::with_capacity(32 * 1024);

    loop {
        let candidate = dir.join("package.json");
        if let Ok(mut file) = File::open(&candidate) {
            buf.clear();
            if file.read_to_string(&mut buf).is_ok() {
                return Some((candidate, buf));
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn main() {
    let mut args = env::args_os().skip(1);

    let script_name = match args.next() {
        Some(s) => s,
        None => {
            // List mode
            let (_, content) = match find_and_read_package_json() {
                Some(r) => r,
                None => {
                    eprintln!("No package.json found");
                    exit(1);
                }
            };

            let scripts = extract_script_names(&content);
            if scripts.is_empty() {
                println!("No scripts defined");
            } else {
                println!("Scripts available via `nr <name>`:\n");
                for (name, cmd) in scripts {
                    println!("  \x1b[1;36m{name}\x1b[0m");
                    println!("    \x1b[2m{cmd}\x1b[0m\n");
                }
            }
            return;
        }
    };

    let script_name = script_name.to_string_lossy();

    let (pkg_path, content) = match find_and_read_package_json() {
        Some(r) => r,
        None => {
            eprintln!("No package.json found");
            exit(1);
        }
    };

    let pkg_dir = pkg_path.parent().unwrap();

    let script_cmd = match extract_script(&content, &script_name) {
        Some(cmd) => cmd,
        None => {
            eprintln!("Script '{}' not found", script_name);
            let scripts = extract_script_names(&content);
            if !scripts.is_empty() {
                eprintln!("Available: {}", scripts.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", "));
            }
            exit(1);
        }
    };

    // Build the full command with extra args
    let extra_args: Vec<_> = args.collect();
    let full_cmd = if extra_args.is_empty() {
        script_cmd.to_string()
    } else {
        let args_str: Vec<_> = extra_args.iter().map(|s| s.to_string_lossy()).collect();
        format!("{} {}", script_cmd, args_str.join(" "))
    };

    // Build PATH - only if node_modules/.bin exists
    let bin_dir = pkg_dir.join("node_modules/.bin");
    let path: OsString = if bin_dir.is_dir() {
        match env::var_os("PATH") {
            Some(p) => {
                let mut new_path = bin_dir.into_os_string();
                new_path.push(":");
                new_path.push(&p);
                new_path
            }
            None => bin_dir.into_os_string(),
        }
    } else {
        env::var_os("PATH").unwrap_or_default()
    };

    // Print command and flush before exec
    {
        let mut stdout = std::io::stdout().lock();
        let _ = writeln!(stdout, "\x1b[2m$\x1b[0m {}", script_cmd);
        let _ = stdout.flush();
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // For simple commands (no shell metacharacters), exec directly
        let needs_shell = full_cmd.contains(['|', '&', ';', '<', '>', '(', ')', '$', '`', '"', '\'', '\\', '\n', '*', '?', '[', ']', '#', '~', '!', '{', '}']);

        let err = if needs_shell {
            Command::new("sh")
                .arg("-c")
                .arg(&full_cmd)
                .current_dir(pkg_dir)
                .env("PATH", path)
                .exec()
        } else {
            // Parse command into program and args
            let mut parts = full_cmd.split_whitespace();
            let program = parts.next().unwrap_or("true");
            let args: Vec<&str> = parts.collect();

            Command::new(program)
                .args(&args)
                .current_dir(pkg_dir)
                .env("PATH", path)
                .exec()
        };
        eprintln!("Failed to exec: {err}");
        exit(1);
    }

    #[cfg(windows)]
    {
        let status = Command::new("cmd")
            .args(["/C", &full_cmd])
            .current_dir(pkg_dir)
            .env("PATH", path)
            .status()
            .unwrap_or_else(|e| {
                eprintln!("Failed to run command: {e}");
                exit(1);
            });
        exit(status.code().unwrap_or(1));
    }
}
