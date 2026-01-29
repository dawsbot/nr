mod completions;

use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process::{exit, Command};

#[derive(Deserialize)]
pub struct PackageJson {
    pub scripts: Option<HashMap<String, String>>,
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

fn print_usage() {
    println!("nr - Run npm scripts. 28x faster.\n");
    println!("Usage:");
    println!("  nr                          List available scripts");
    println!("  nr <script> [args...]       Run a script");
    println!("  nr --completions <shell>    Generate shell completions (bash, zsh, fish)");
    println!("  nr --help                   Show this help");
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    // Handle flags before looking for package.json
    if let Some(first) = args.first() {
        match first.as_str() {
            "--help" | "-h" => {
                print_usage();
                return;
            }
            "--completions" => {
                let shell = args.get(1).unwrap_or_else(|| {
                    eprintln!("Usage: nr --completions <bash|zsh|fish>");
                    exit(1);
                });
                println!("{}", completions::generate(shell));
                return;
            }
            "--list-scripts" => {
                completions::list_script_names();
                return;
            }
            _ => {}
        }
    }

    let pkg_path = match find_package_json() {
        Some(p) => p,
        None => {
            eprintln!("No package.json found");
            exit(1);
        }
    };

    let pkg_dir = pkg_path.parent().unwrap();

    let content = fs::read_to_string(&pkg_path).unwrap_or_else(|e| {
        eprintln!("Failed to read package.json: {e}");
        exit(1);
    });

    let pkg: PackageJson = serde_json::from_str(&content).unwrap_or_else(|e| {
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
        entries.sort_by_key(|(k, _)| *k);
        for (name, cmd) in entries {
            println!("  \x1b[1;36m{name}\x1b[0m");
            println!("    \x1b[2m{cmd}\x1b[0m\n");
        }
        return;
    }

    let script_name = &args[0];
    let extra_args = &args[1..];

    let script_cmd = match scripts.get(script_name) {
        Some(cmd) => cmd,
        None => {
            eprintln!("Script '{script_name}' not found");
            eprintln!("Available: {}", scripts.keys().cloned().collect::<Vec<_>>().join(", "));
            exit(1);
        }
    };

    let full_cmd = if extra_args.is_empty() {
        script_cmd.clone()
    } else {
        format!("{} {}", script_cmd, extra_args.join(" "))
    };

    // Build PATH with all node_modules/.bin directories (walk up for monorepos)
    let mut bin_paths: Vec<PathBuf> = Vec::new();
    let mut search_dir = pkg_dir.to_path_buf();
    loop {
        let bin_dir = search_dir.join("node_modules/.bin");
        if bin_dir.is_dir() {
            bin_paths.push(bin_dir);
        }
        if !search_dir.pop() {
            break;
        }
    }

    let path: OsString = if bin_paths.is_empty() {
        env::var_os("PATH").unwrap_or_default()
    } else {
        let mut new_path = OsString::new();
        for (i, bin_path) in bin_paths.iter().enumerate() {
            if i > 0 {
                new_path.push(":");
            }
            new_path.push(bin_path);
        }
        if let Some(existing) = env::var_os("PATH") {
            new_path.push(":");
            new_path.push(&existing);
        }
        new_path
    };

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
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
        let status = Command::new("cmd")
            .args(["/C", &full_cmd])
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
