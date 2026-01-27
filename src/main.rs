use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{exit, Command};
use std::time::SystemTime;

const CACHE_FILE: &str = ".nr-cache";

/// Cache format: first line is mtime (secs.nanos), rest is name\tcommand per line
struct ScriptCache {
    mtime_secs: u64,
    mtime_nanos: u32,
    scripts: Vec<(String, String)>,
}

impl ScriptCache {
    fn load(cache_path: &Path) -> Option<Self> {
        // Single read, no buffering overhead
        let content = fs::read_to_string(cache_path).ok()?;
        let mut lines = content.lines();

        // First line: mtime
        let mtime_str = lines.next()?;
        let (secs_str, nanos_str) = mtime_str.split_once('.')?;
        let mtime_secs: u64 = secs_str.parse().ok()?;
        let mtime_nanos: u32 = nanos_str.parse().ok()?;

        // Rest: scripts
        let mut scripts = Vec::new();
        for line in lines {
            let (name, cmd) = line.split_once('\t')?;
            scripts.push((name.to_string(), cmd.to_string()));
        }

        Some(ScriptCache { mtime_secs, mtime_nanos, scripts })
    }

    fn save(&self, cache_path: &Path) {
        let mut content = format!("{}.{}\n", self.mtime_secs, self.mtime_nanos);
        for (name, cmd) in &self.scripts {
            content.push_str(name);
            content.push('\t');
            content.push_str(cmd);
            content.push('\n');
        }
        let _ = fs::write(cache_path, content);
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.scripts.iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| c.as_str())
    }
}

fn get_mtime(path: &Path) -> Option<(u64, u32)> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    let duration = mtime.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    Some((duration.as_secs(), duration.subsec_nanos()))
}

/// Extract script from JSON content
fn extract_script<'a>(content: &'a str, name: &str) -> Option<&'a str> {
    let scripts_start = content.find("\"scripts\"")?;
    let after_scripts = &content[scripts_start..];
    let brace_pos = after_scripts.find('{')?;
    let scripts_content = &after_scripts[brace_pos..];

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

    let search_key = format!("\"{}\"", name);
    let key_pos = scripts_block.find(&search_key)?;
    let after_key = &scripts_block[key_pos + search_key.len()..];
    let colon_pos = after_key.find(':')?;
    let after_colon = after_key[colon_pos + 1..].trim_start();

    if !after_colon.starts_with('"') {
        return None;
    }

    let value_content = &after_colon[1..];
    let mut end = 0;
    let mut escape = false;
    for (i, c) in value_content.char_indices() {
        if escape { escape = false; continue; }
        if c == '\\' { escape = true; continue; }
        if c == '"' { end = i; break; }
    }

    Some(&value_content[..end])
}

/// Extract all scripts from JSON content
fn extract_all_scripts(content: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();

    let Some(scripts_start) = content.find("\"scripts\"") else { return results; };
    let after_scripts = &content[scripts_start..];
    let Some(brace_pos) = after_scripts.find('{') else { return results; };
    let scripts_content = &after_scripts[brace_pos..];

    let mut depth = 0;
    let mut scripts_end = 0;
    for (i, c) in scripts_content.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => { depth -= 1; if depth == 0 { scripts_end = i; break; } }
            _ => {}
        }
    }
    let scripts_block = &scripts_content[1..scripts_end];

    let mut pos = 0;
    while pos < scripts_block.len() {
        let Some(key_start) = scripts_block[pos..].find('"') else { break; };
        let key_start = pos + key_start + 1;
        let Some(key_end) = scripts_block[key_start..].find('"') else { break; };
        let key_end = key_start + key_end;
        let key = &scripts_block[key_start..key_end];

        let after_key = &scripts_block[key_end + 1..];
        let Some(colon) = after_key.find(':') else { break; };
        let after_colon = &after_key[colon + 1..];
        let Some(val_start) = after_colon.find('"') else { break; };
        let val_start_abs = key_end + 1 + colon + 1 + val_start + 1;

        let val_content = &scripts_block[val_start_abs..];
        let mut val_end = 0;
        let mut escape = false;
        for (i, c) in val_content.char_indices() {
            if escape { escape = false; continue; }
            if c == '\\' { escape = true; continue; }
            if c == '"' { val_end = i; break; }
        }

        results.push((key.to_string(), val_content[..val_end].to_string()));
        pos = val_start_abs + val_end + 1;
    }

    results
}

fn find_package_json() -> Option<PathBuf> {
    let mut dir = env::current_dir().ok()?;
    loop {
        let candidate = dir.join("package.json");
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Try to get script from cache, returns (script_cmd, pkg_dir, all_scripts_for_error)
fn get_script_cached(pkg_path: &Path, script_name: &str) -> Result<(String, Vec<(String, String)>), String> {
    let cache_path = pkg_path.parent().unwrap().join(CACHE_FILE);

    // Compare file mtimes - if cache is newer than package.json, it's valid
    // This avoids reading the cache just to check validity
    let pkg_meta = fs::metadata(pkg_path).ok();
    let cache_meta = fs::metadata(&cache_path).ok();

    let cache_valid = match (&pkg_meta, &cache_meta) {
        (Some(pkg), Some(cache)) => {
            match (pkg.modified(), cache.modified()) {
                (Ok(pkg_time), Ok(cache_time)) => cache_time >= pkg_time,
                _ => false,
            }
        }
        _ => false,
    };

    if cache_valid {
        // Read cache - it's valid
        if let Some(cache) = ScriptCache::load(&cache_path) {
            if let Some(cmd) = cache.get(script_name) {
                return Ok((cmd.to_string(), cache.scripts));
            } else {
                return Err(format!("Script '{}' not found", script_name));
            }
        }
    }

    // Cache miss or invalid - read and parse package.json
    let content = fs::read_to_string(pkg_path)
        .map_err(|e| format!("Failed to read package.json: {}", e))?;

    let scripts = extract_all_scripts(&content);

    // Save cache (mtime fields unused now, but keep for compatibility)
    let cache = ScriptCache {
        mtime_secs: 0,
        mtime_nanos: 0,
        scripts: scripts.clone(),
    };
    cache.save(&cache_path);

    // Find the script
    if let Some((_, cmd)) = scripts.iter().find(|(n, _)| n == script_name) {
        Ok((cmd.clone(), scripts))
    } else {
        Err(format!("Script '{}' not found", script_name))
    }
}

fn main() {
    let mut args = env::args_os().skip(1);

    let script_name = match args.next() {
        Some(s) => s,
        None => {
            // List mode - always read fresh for listing
            let pkg_path = match find_package_json() {
                Some(p) => p,
                None => { eprintln!("No package.json found"); exit(1); }
            };

            let cache_path = pkg_path.parent().unwrap().join(CACHE_FILE);
            let pkg_mtime = get_mtime(&pkg_path);

            // Try cache
            let scripts = if let (Some((secs, nanos)), Some(cache)) = (pkg_mtime, ScriptCache::load(&cache_path)) {
                if cache.mtime_secs == secs && cache.mtime_nanos == nanos {
                    cache.scripts
                } else {
                    let content = fs::read_to_string(&pkg_path).unwrap_or_default();
                    let scripts = extract_all_scripts(&content);
                    if let Some((s, n)) = pkg_mtime {
                        ScriptCache { mtime_secs: s, mtime_nanos: n, scripts: scripts.clone() }.save(&cache_path);
                    }
                    scripts
                }
            } else {
                let content = fs::read_to_string(&pkg_path).unwrap_or_default();
                let scripts = extract_all_scripts(&content);
                if let Some((s, n)) = pkg_mtime {
                    ScriptCache { mtime_secs: s, mtime_nanos: n, scripts: scripts.clone() }.save(&cache_path);
                }
                scripts
            };

            if scripts.is_empty() {
                println!("No scripts defined");
            } else {
                println!("Scripts available via `nr <name>`:\n");
                let mut sorted = scripts;
                sorted.sort_by(|a, b| a.0.cmp(&b.0));
                for (name, cmd) in sorted {
                    println!("  \x1b[1;36m{name}\x1b[0m");
                    println!("    \x1b[2m{cmd}\x1b[0m\n");
                }
            }
            return;
        }
    };

    let script_name = script_name.to_string_lossy();

    let pkg_path = match find_package_json() {
        Some(p) => p,
        None => { eprintln!("No package.json found"); exit(1); }
    };

    let pkg_dir = pkg_path.parent().unwrap();

    let (script_cmd, all_scripts) = match get_script_cached(&pkg_path, &script_name) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{}", e);
            // Try to show available scripts
            if let Ok(content) = fs::read_to_string(&pkg_path) {
                let scripts = extract_all_scripts(&content);
                if !scripts.is_empty() {
                    eprintln!("Available: {}", scripts.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(", "));
                }
            }
            exit(1);
        }
    };

    // Build the full command with extra args
    let extra_args: Vec<_> = args.collect();
    let full_cmd = if extra_args.is_empty() {
        script_cmd.clone()
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

        let needs_shell = full_cmd.contains(['|', '&', ';', '<', '>', '(', ')', '$', '`', '"', '\'', '\\', '\n', '*', '?', '[', ']', '#', '~', '!', '{', '}']);

        let err = if needs_shell {
            Command::new("sh")
                .arg("-c")
                .arg(&full_cmd)
                .current_dir(pkg_dir)
                .env("PATH", path)
                .exec()
        } else {
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
