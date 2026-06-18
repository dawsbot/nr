mod sources;

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

use sources::{Detection, RunSpec, SourceKind};

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

/// Build a PATH with every `node_modules/.bin` from `dir` up to the root, so npm
/// scripts can find their locally installed binaries (including in monorepos).
fn path_with_node_bins(dir: &Path) -> OsString {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut search_dir = dir.to_path_buf();
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
    if paths.is_empty() {
        existing_path
    } else {
        paths.extend(env::split_paths(&existing_path));
        env::join_paths(paths).unwrap_or(existing_path)
    }
}

fn list(detection: &Detection) {
    println!("Run a task with `nr <name>`:\n");
    let mut current: Option<SourceKind> = None;
    for task in &detection.tasks {
        if current != Some(task.source) {
            current = Some(task.source);
            println!("  \x1b[2m{}\x1b[0m", task.source.label());
        }
        println!("    \x1b[1;36m{}\x1b[0m", task.name);
        println!("      \x1b[2m{}\x1b[0m", task.detail);
    }
    println!();
}

/// Run a raw command line (npm script, Procfile entry, taskipy task), reusing
/// the original zero-overhead path: exec directly when no shell is needed.
fn run_shell(cmd: &str, dir: &Path, extra_args: &[OsString]) -> ! {
    exec_command_line(cmd, dir, path_with_node_bins(dir), None, extra_args)
}

/// Run a command inside a Python virtualenv: prepend its `bin/` to PATH and
/// export `VIRTUAL_ENV`, then exec directly. This is how `nr` replaces a
/// `poetry run` / `pdm run` / `pipenv run` launch without paying the Python
/// interpreter startup those tools incur.
fn run_in_venv(cmd: &str, bin: &Path, dir: &Path, extra_args: &[OsString]) -> ! {
    let existing = env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![bin.to_path_buf()];
    dirs.extend(env::split_paths(&existing));
    let path = env::join_paths(dirs).unwrap_or(existing);
    exec_command_line(cmd, dir, path, bin.parent(), extra_args)
}

/// Shared executor for command-line tasks: exec the program directly when it has
/// no shell metacharacters (preserving args verbatim), otherwise hand it to the
/// shell. `virtual_env`, when set, is exported as `VIRTUAL_ENV`.
fn exec_command_line(
    cmd: &str,
    dir: &Path,
    path: OsString,
    virtual_env: Option<&Path>,
    extra_args: &[OsString],
) -> ! {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        if is_simple_command(cmd) {
            let mut parts = cmd.split_ascii_whitespace();
            let prog = parts.next().unwrap();
            let mut command = Command::new(prog);
            command
                .args(parts)
                .args(extra_args)
                .current_dir(dir)
                .env("PATH", &path);
            if let Some(ve) = virtual_env {
                command.env("VIRTUAL_ENV", ve);
            }
            let _ = command.exec();
            // exec only returns on failure; fall through to the shell so it can
            // emit its usual diagnostics.
        }

        let mut full_cmd = OsString::from(cmd);
        for arg in extra_args {
            full_cmd.push(" ");
            full_cmd.push(arg);
        }
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(&full_cmd)
            .current_dir(dir)
            .env("PATH", path);
        if let Some(ve) = virtual_env {
            command.env("VIRTUAL_ENV", ve);
        }
        let err = command.exec();
        eprintln!("Failed to exec: {err}");
        exit(1);
    }

    #[cfg(windows)]
    {
        let mut full_cmd = OsString::from(cmd);
        for arg in extra_args {
            full_cmd.push(" ");
            full_cmd.push(arg);
        }
        let mut command = Command::new("cmd");
        command
            .arg("/C")
            .arg(&full_cmd)
            .current_dir(dir)
            .env("PATH", path);
        if let Some(ve) = virtual_env {
            command.env("VIRTUAL_ENV", ve);
        }
        let status = command.status().unwrap_or_else(|e| {
            eprintln!("Failed to run: {e}");
            exit(1);
        });
        exit(status.code().unwrap_or(1));
    }
}

/// Exec a delegated tool (`make`, `just`, `cargo`, `task`, `pdm`, `poetry`),
/// appending the user's extra args.
fn run_exec(argv: &[String], dir: &Path, extra_args: &[OsString]) -> ! {
    let (prog, rest) = argv.split_first().unwrap_or_else(|| {
        eprintln!("Invalid task definition");
        exit(1);
    });

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = Command::new(prog)
            .args(rest)
            .args(extra_args)
            .current_dir(dir)
            .exec();
        eprintln!("Failed to exec `{prog}`: {err}");
        eprintln!("Is `{prog}` installed and on your PATH?");
        exit(1);
    }

    #[cfg(windows)]
    {
        let status = Command::new(prog)
            .args(rest)
            .args(extra_args)
            .current_dir(dir)
            .status()
            .unwrap_or_else(|e| {
                eprintln!("Failed to run `{prog}`: {e}");
                eprintln!("Is `{prog}` installed and on your PATH?");
                exit(1);
            });
        exit(status.code().unwrap_or(1));
    }
}

fn main() {
    let args: Vec<OsString> = env::args_os().skip(1).collect();

    let detection = match sources::detect() {
        Some(d) => d,
        None => {
            eprintln!(
                "No tasks found. Looked for package.json, Makefile, justfile,\n\
                 Cargo.toml, pyproject.toml, Procfile and Taskfile up the tree."
            );
            exit(1);
        }
    };

    if detection.tasks.is_empty() {
        let found: Vec<&str> = detection.manifests.iter().map(|m| m.label()).collect();
        eprintln!(
            "Found {} but no runnable tasks were defined.",
            found.join(", ")
        );
        exit(1);
    }

    // No args: list available tasks.
    if args.is_empty() {
        list(&detection);
        return;
    }

    let task_name = match args[0].to_str() {
        Some(s) => s,
        None => {
            eprintln!("Task name is not valid UTF-8");
            exit(1);
        }
    };
    let extra_args = &args[1..];

    let task = match detection.find(task_name) {
        Some(t) => t,
        None => {
            eprintln!("Task '{task_name}' not found");
            let mut names: Vec<&str> = detection.tasks.iter().map(|t| t.name.as_str()).collect();
            names.sort();
            names.dedup();
            eprintln!("Available: {}", names.join(", "));
            exit(1);
        }
    };

    match &task.run {
        RunSpec::Shell(cmd) => run_shell(cmd, &detection.dir, extra_args),
        RunSpec::Exec(argv) => run_exec(argv, &detection.dir, extra_args),
        RunSpec::Venv {
            command,
            requires,
            fallback,
        } => {
            // Take the venv fast path only when we found a local environment and
            // (for poetry console scripts) the entry point actually exists in it.
            let bin = sources::find_venv_bin(&detection.dir).filter(|bin| match requires {
                Some(prog) => bin.join(prog).exists(),
                None => true,
            });
            match bin {
                Some(bin) => run_in_venv(command, &bin, &detection.dir, extra_args),
                None => run_exec(fallback, &detection.dir, extra_args),
            }
        }
    }
}
