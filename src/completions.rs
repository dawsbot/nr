use std::env;
use std::fs;

use crate::PackageJson;

/// Print script names from the nearest package.json (one per line).
/// Used by bash completion scripts to dynamically complete script names.
pub fn list_script_names() {
    if let Some(scripts) = get_scripts() {
        let mut names: Vec<&String> = scripts.keys().collect();
        names.sort();
        for name in names {
            println!("{name}");
        }
    }
}

/// Print script names with their commands in `name:command` format (one per line).
/// Used by zsh and fish completion scripts to show descriptions alongside completions.
pub fn list_script_names_detailed() {
    if let Some(scripts) = get_scripts() {
        let mut entries: Vec<(&String, &String)> = scripts.iter().collect();
        entries.sort_by_key(|(k, _)| *k);
        for (name, cmd) in entries {
            // Escape colons since zsh uses : as the name:description delimiter
            let escaped_name = name.replace(':', "\\:");
            let escaped_cmd = cmd.replace(':', "\\:");
            println!("{escaped_name}:{escaped_cmd}");
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
        # Remove colon from WORDBREAKS so scripts like "test:nr" complete correctly
        COMP_WORDBREAKS="${COMP_WORDBREAKS//:}"
        local scripts
        scripts="$(nr --list-scripts 2>/dev/null)"
        COMPREPLY=($(compgen -W "$scripts" -- "$cur"))
    fi
}
complete -F _nr_completions nr"#
        .to_string()
}

fn generate_zsh() -> String {
    r#"_nr() {
    local -a scripts
    scripts=(${(f)"$(nr --list-scripts-detailed 2>/dev/null)"})
    if (( CURRENT == 2 )); then
        _describe 'script' scripts
    fi
}
compdef _nr nr"#
        .to_string()
}

fn generate_fish() -> String {
    r#"complete -c nr -f
complete -c nr -n '__fish_use_subcommand' -a '(nr --list-scripts-detailed 2>/dev/null | while read -l line; set -l parts (string split -m1 ":" -- $line); echo $parts[1]\t$parts[2]; end)'"#
        .to_string()
}
