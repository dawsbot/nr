use std::env;
use std::fs;

use crate::PackageJson;

/// Print script names from the nearest package.json (one per line).
/// Used by shell completion scripts to dynamically complete script names.
pub fn list_script_names() {
    if let Some(scripts) = get_scripts() {
        let mut names: Vec<&String> = scripts.keys().collect();
        names.sort();
        for name in names {
            println!("{name}");
        }
    }
}

fn get_scripts() -> Option<std::collections::HashMap<String, String>> {
    let mut dir = env::current_dir().ok()?;
    loop {
        let candidate = dir.join("package.json");
        if candidate.exists() {
            let content = fs::read_to_string(&candidate).ok()?;
            let pkg: PackageJson = serde_json::from_str(&content).ok()?;
            return pkg.scripts;
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub fn generate(shell: &str) -> String {
    match shell {
        "bash" => generate_bash(),
        "zsh" => generate_zsh(),
        "fish" => generate_fish(),
        _ => {
            eprintln!("Unsupported shell: {shell}. Use bash, zsh, or fish.");
            std::process::exit(1);
        }
    }
}

fn generate_bash() -> String {
    r#"_nr_completions() {
    local cur="${COMP_WORDS[COMP_CWORD]}"
    if [ "$COMP_CWORD" -eq 1 ]; then
        local scripts
        scripts="$(nr --list-scripts 2>/dev/null)"
        COMPREPLY=($(compgen -W "$scripts" -- "$cur"))
    fi
}
complete -F _nr_completions nr"#
        .to_string()
}

fn generate_zsh() -> String {
    r#"#compdef nr

_nr() {
    local -a scripts
    scripts=(${(f)"$(nr --list-scripts 2>/dev/null)"})
    if (( CURRENT == 2 )); then
        _describe 'script' scripts
    fi
}

_nr "$@""#
        .to_string()
}

fn generate_fish() -> String {
    r#"complete -c nr -f
complete -c nr -n '__fish_use_subcommand' -a '(nr --list-scripts 2>/dev/null)' -d 'npm script'"#
        .to_string()
}
