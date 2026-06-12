use serde::Deserialize;
use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process::{exit, Command};

#[derive(Deserialize)]
struct PackageJson<'a> {
    #[serde(borrow)]
    scripts: Option<HashMap<Cow<'a, str>, Cow<'a, str>>>,
}

fn find_package_json() -> Option<(PathBuf, Vec<u8>)> {
    let mut dir = env::current_dir().ok()?;
    loop {
        let candidate = dir.join("package.json");
        if let Ok(content) = fs::read(&candidate) {
            return Some((candidate, content));
        }
        if !dir.pop() {
            return None;
        }
    }
}

// A command is safe to exec directly (skipping the shell) if it has no shell
// metacharacters and no leading VAR=value assignment.
fn is_simple_command(cmd: &str) -> bool {
    match cmd.split_ascii_whitespace().next() {
        Some(first) if !first.contains('=') => {}
        _ => return false,
    }
    !cmd.bytes()
        .any(|b| b"&|;<>$`\"'(){}[]*?~!#\\\n\r".contains(&b))
}

fn main() {
    let args: Vec<OsString> = env::args_os().skip(1).collect();

    let (pkg_path, content) = match find_package_json() {
        Some(found) => found,
        None => {
            eprintln!("No package.json found");
            exit(1);
        }
    };

    let pkg_dir = pkg_path.parent().unwrap();

    let pkg: PackageJson = serde_json::from_slice(&content).unwrap_or_else(|e| {
        eprintln!("Failed to parse package.json: {e}");
        exit(1);
    });

    let scripts = match pkg.scripts {
        Some(s) => s,
        None => {
            eprintln!("No scripts defined");
            exit(1);
        }
    };

    // No args: list available scripts
    if args.is_empty() {
        println!("Scripts available via `nr <name>`:\n");
        let mut entries: Vec<_> = scripts.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        for (name, cmd) in entries {
            println!("  \x1b[1;36m{name}\x1b[0m");
            println!("    \x1b[2m{cmd}\x1b[0m\n");
        }
        return;
    }

    let script_name = match args[0].to_str() {
        Some(s) => s,
        None => {
            eprintln!("Script name is not valid UTF-8");
            exit(1);
        }
    };
    let extra_args = &args[1..];

    let script_cmd: &str = match scripts.get(script_name) {
        Some(cmd) => cmd,
        None => {
            eprintln!("Script '{script_name}' not found");
            let mut names: Vec<&str> = scripts.keys().map(|k| k.as_ref()).collect();
            names.sort();
            eprintln!("Available: {}", names.join(", "));
            exit(1);
        }
    };

    // Build PATH with all node_modules/.bin directories (walk up for monorepos)
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut search_dir = pkg_dir.to_path_buf();
    loop {
        let bin_dir = search_dir.join("node_modules/.bin");
        if bin_dir.is_dir() {
            paths.push(bin_dir);
        }
        if !search_dir.pop() {
            break;
        }
    }

    let existing_path = env::var_os("PATH").unwrap_or_default();
    let path: OsString = if paths.is_empty() {
        existing_path
    } else {
        paths.extend(env::split_paths(&existing_path));
        env::join_paths(paths).unwrap_or(existing_path)
    };

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // Fast path: no shell metacharacters means no shell is needed.
        // Exec the program directly and pass extra args as real argv
        // entries, which also preserves args containing spaces.
        if is_simple_command(script_cmd) {
            let mut parts = script_cmd.split_ascii_whitespace();
            let prog = parts.next().unwrap();
            let _ = Command::new(prog)
                .args(parts)
                .args(extra_args)
                .current_dir(pkg_dir)
                .env("PATH", &path)
                .exec();
            // exec only returns on failure; fall through so the shell
            // produces its usual "command not found" diagnostics.
        }

        let mut full_cmd = OsString::from(script_cmd);
        for arg in extra_args {
            full_cmd.push(" ");
            full_cmd.push(arg);
        }
        let err = Command::new("sh")
            .arg("-c")
            .arg(&full_cmd)
            .current_dir(pkg_dir)
            .env("PATH", path)
            .exec();
        eprintln!("Failed to exec: {err}");
        exit(1);
    }

    #[cfg(windows)]
    {
        let mut full_cmd = OsString::from(script_cmd);
        for arg in extra_args {
            full_cmd.push(" ");
            full_cmd.push(arg);
        }
        let status = Command::new("cmd")
            .arg("/C")
            .arg(&full_cmd)
            .current_dir(pkg_dir)
            .env("PATH", path)
            .status()
            .unwrap_or_else(|e| {
                eprintln!("Failed to run: {e}");
                exit(1);
            });
        exit(status.code().unwrap_or(1));
    }
}
