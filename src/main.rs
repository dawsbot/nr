use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{exit, Command};

#[derive(Deserialize)]
struct PackageJson {
    scripts: Option<HashMap<String, String>>,
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

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        // List available scripts
        let pkg_path = match find_package_json() {
            Some(p) => p,
            None => {
                eprintln!("No package.json found");
                exit(1);
            }
        };

        let content = fs::read_to_string(&pkg_path).unwrap_or_else(|e| {
            eprintln!("Failed to read package.json: {e}");
            exit(1);
        });

        let pkg: PackageJson = serde_json::from_str(&content).unwrap_or_else(|e| {
            eprintln!("Failed to parse package.json: {e}");
            exit(1);
        });

        match pkg.scripts {
            Some(scripts) if !scripts.is_empty() => {
                println!("Scripts available via `nr <name>`:\n");
                let mut entries: Vec<_> = scripts.iter().collect();
                entries.sort_by_key(|(k, _)| *k);
                for (name, cmd) in entries {
                    // Bold cyan for name, dim for command
                    println!("  \x1b[1;36m{name}\x1b[0m");
                    println!("    \x1b[2m{cmd}\x1b[0m\n");
                }
            }
            _ => println!("No scripts defined"),
        }
        return;
    }

    let script_name = &args[0];
    let extra_args = &args[1..];

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
            eprintln!("No scripts defined in package.json");
            exit(1);
        }
    };

    let script_cmd = match scripts.get(script_name) {
        Some(cmd) => cmd,
        None => {
            eprintln!("Script '{script_name}' not found");
            eprintln!("Available: {}", scripts.keys().cloned().collect::<Vec<_>>().join(", "));
            exit(1);
        }
    };

    // Append extra arguments to the command
    let full_cmd = if extra_args.is_empty() {
        script_cmd.clone()
    } else {
        format!("{} {}", script_cmd, extra_args.join(" "))
    };

    // Print the command being run (like bun)
    println!("\x1b[2m$\x1b[0m {}", script_cmd);

    // Build PATH with node_modules/.bin prepended
    let bin_dir = pkg_dir.join("node_modules/.bin");
    let path = match env::var_os("PATH") {
        Some(p) => {
            let mut paths = env::split_paths(&p).collect::<Vec<_>>();
            paths.insert(0, bin_dir);
            env::join_paths(paths).unwrap()
        }
        None => bin_dir.into_os_string(),
    };

    // On Unix, use exec to replace the process (fastest)
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

    // On Windows, spawn and wait
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
