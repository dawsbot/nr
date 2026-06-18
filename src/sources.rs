//! Task discovery across the manifests a project might use.
//!
//! `nr` started as an npm-script runner. This module generalises it: from the
//! nearest directory containing a recognised manifest we collect every runnable
//! task, regardless of which tool defines it. Parsing is deliberately minimal,
//! no new dependencies, just enough to learn task *names* (and a command preview
//! for display). Execution is delegated to the canonical tool for each source
//! (`make`, `just`, `cargo`, `task`) so we never reimplement their build logic,
//! and the whole thing stays a single `exec()` on the hot path. The exception is
//! the Python launchers (`poetry`, `pdm`, `pipenv`), which bootstrap an
//! interpreter on every `run`: for those we resolve the project virtualenv and
//! exec the command directly (see [`RunSpec::Venv`]), falling back to the
//! launcher only when no local environment is found.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// How to run a discovered task.
#[derive(Debug, Clone, PartialEq)]
pub enum RunSpec {
    /// A raw command line, run through the existing shell-or-direct-exec logic
    /// with `node_modules/.bin` injected onto PATH (npm scripts, Procfile
    /// entries, taskipy tasks).
    Shell(String),
    /// A fixed argv prefix to exec directly; the user's extra args are appended.
    /// Used to delegate to a project tool, e.g. `["make", "build"]`.
    Exec(Vec<String>),
    /// Run a command inside the project's Python virtualenv, skipping the
    /// launcher's interpreter startup. When a local environment can be found
    /// (an active `$VIRTUAL_ENV`, an in-project `.venv`, or pdm's
    /// `__pypackages__`) we put its `bin/` on PATH and exec `command` directly.
    /// Otherwise we fall back to `fallback` (e.g. `["poetry", "run", "lint"]`)
    /// so behaviour stays correct even when we can't locate the env.
    Venv {
        command: String,
        /// When set, the fast path is only taken if this executable exists in
        /// the resolved virtualenv `bin/`. Poetry console scripts set this to
        /// the script name, since they only run natively once installed;
        /// otherwise we delegate. Plain shell commands (pdm/pipenv) leave it
        /// `None` — they can use the venv but don't require an entry point.
        requires: Option<String>,
        fallback: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Npm,
    Make,
    Just,
    Cargo,
    Python,
    Pipenv,
    Procfile,
    Taskfile,
}

impl SourceKind {
    /// Lower-case label shown as a group header when listing tasks.
    pub fn label(self) -> &'static str {
        match self {
            SourceKind::Npm => "package.json",
            SourceKind::Make => "Makefile",
            SourceKind::Just => "justfile",
            SourceKind::Cargo => "Cargo.toml",
            SourceKind::Python => "pyproject.toml",
            SourceKind::Pipenv => "Pipfile",
            SourceKind::Procfile => "Procfile",
            SourceKind::Taskfile => "Taskfile",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Task {
    pub name: String,
    /// Short preview shown when listing (command line or a hint).
    pub detail: String,
    pub source: SourceKind,
    pub run: RunSpec,
}

/// The result of walking up the tree to find a project.
pub struct Detection {
    /// Directory the tasks were found in; commands run here.
    pub dir: PathBuf,
    /// Manifest files present in `dir` (even if they yielded no tasks).
    pub manifests: Vec<SourceKind>,
    /// Every runnable task, in source-precedence order.
    pub tasks: Vec<Task>,
}

impl Detection {
    /// First task matching `name`, honouring source precedence.
    pub fn find(&self, name: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.name == name)
    }
}

/// Walk up from `start` to the filesystem root, returning the first directory
/// that contains at least one recognised manifest.
pub fn detect_from(start: &Path) -> Option<Detection> {
    let mut dir = start.to_path_buf();
    loop {
        let (manifests, tasks) = collect(&dir);
        if !manifests.is_empty() {
            return Some(Detection {
                dir,
                manifests,
                tasks,
            });
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Walk up from the current working directory.
pub fn detect() -> Option<Detection> {
    let cwd = env::current_dir().ok()?;
    detect_from(&cwd)
}

/// Read every recognised manifest in `dir` and merge their tasks. The order of
/// the sources here defines name-resolution precedence and listing order.
fn collect(dir: &Path) -> (Vec<SourceKind>, Vec<Task>) {
    let mut manifests = Vec::new();
    let mut tasks = Vec::new();
    let mut seen = HashSet::new();

    let mut add = |kind: SourceKind, found: bool, mut new: Vec<Task>| {
        if found {
            manifests.push(kind);
            for t in new.drain(..) {
                // Keep the first task to claim a name (earlier source wins),
                // but still list every source's tasks for visibility.
                seen.insert(t.name.clone());
                tasks.push(t);
            }
        }
    };

    // npm — read package.json `scripts`.
    if let Some(content) = read(dir, "package.json") {
        add(SourceKind::Npm, true, parse_npm(&content));
    }

    // Makefile (several conventional spellings).
    if let Some(content) = read_any(dir, &["Makefile", "makefile", "GNUmakefile"]) {
        add(SourceKind::Make, true, parse_makefile(&content));
    }

    // justfile.
    if let Some(content) = read_any(dir, &["justfile", "Justfile", ".justfile"]) {
        add(SourceKind::Just, true, parse_justfile(&content));
    }

    // Cargo.toml — expose the conventional cargo commands.
    if exists(dir, "Cargo.toml") {
        add(SourceKind::Cargo, true, cargo_tasks());
    }

    // pyproject.toml — pdm / poetry / taskipy task tables.
    if let Some(content) = read(dir, "pyproject.toml") {
        add(SourceKind::Python, true, parse_pyproject(&content));
    }

    // Pipfile — pipenv `[scripts]`.
    if let Some(content) = read(dir, "Pipfile") {
        add(SourceKind::Pipenv, true, parse_pipfile(&content));
    }

    // Procfile.
    if let Some(content) = read_any(dir, &["Procfile", "Procfile.dev"]) {
        add(SourceKind::Procfile, true, parse_procfile(&content));
    }

    // Taskfile (go-task).
    if let Some(content) = read_any(
        dir,
        &[
            "Taskfile.yml",
            "Taskfile.yaml",
            "taskfile.yml",
            "taskfile.yaml",
        ],
    ) {
        add(SourceKind::Taskfile, true, parse_taskfile(&content));
    }

    let _ = seen; // reserved for future "shadowed task" diagnostics
    (manifests, tasks)
}

// ---------------------------------------------------------------------------
// file helpers
// ---------------------------------------------------------------------------

fn read(dir: &Path, name: &str) -> Option<String> {
    fs::read_to_string(dir.join(name)).ok()
}

fn read_any(dir: &Path, names: &[&str]) -> Option<String> {
    names.iter().find_map(|n| read(dir, n))
}

fn exists(dir: &Path, name: &str) -> bool {
    dir.join(name).exists()
}

// ---------------------------------------------------------------------------
// virtualenv resolution
// ---------------------------------------------------------------------------

/// Find the `bin/` (or `Scripts/` on Windows) directory of the Python
/// environment a task should run in. An already-activated `$VIRTUAL_ENV` wins;
/// otherwise we look for a project-local environment. Returns `None` when no
/// local environment exists, signalling the caller to fall back to the launcher.
pub fn find_venv_bin(dir: &Path) -> Option<PathBuf> {
    if let Some(active) = env::var_os("VIRTUAL_ENV") {
        if let Some(bin) = venv_bin_dir(Path::new(&active)) {
            return Some(bin);
        }
    }
    local_venv_bin(dir)
}

/// Project-local environments only (no `$VIRTUAL_ENV` consulted): an in-project
/// `.venv` or a pdm `__pypackages__/<version>` PEP 582 layout. Kept pure so it
/// is deterministically testable.
fn local_venv_bin(dir: &Path) -> Option<PathBuf> {
    if let Some(bin) = venv_bin_dir(&dir.join(".venv")) {
        return Some(bin);
    }
    if let Ok(entries) = fs::read_dir(dir.join("__pypackages__")) {
        // `__pypackages__/3.11/bin` — take the first python version present.
        let mut versions: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        versions.sort();
        for v in versions {
            if let Some(bin) = venv_bin_dir(&v) {
                return Some(bin);
            }
        }
    }
    None
}

/// Given a virtualenv root, return its executables directory if it exists.
fn venv_bin_dir(root: &Path) -> Option<PathBuf> {
    for sub in ["bin", "Scripts"] {
        let candidate = root.join(sub);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

fn truncate(s: &str) -> String {
    let s = s.trim();
    const MAX: usize = 64;
    if s.chars().count() > MAX {
        let mut out: String = s.chars().take(MAX - 1).collect();
        out.push('…');
        out
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// parsers
// ---------------------------------------------------------------------------

fn parse_npm(content: &str) -> Vec<Task> {
    #[derive(serde::Deserialize)]
    struct Pkg {
        scripts: Option<std::collections::HashMap<String, String>>,
    }
    let pkg: Pkg = match serde_json::from_str(content) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let mut tasks: Vec<Task> = pkg
        .scripts
        .unwrap_or_default()
        .into_iter()
        .map(|(name, cmd)| Task {
            detail: truncate(&cmd),
            name,
            source: SourceKind::Npm,
            run: RunSpec::Shell(cmd),
        })
        .collect();
    tasks.sort_by(|a, b| a.name.cmp(&b.name));
    tasks
}

/// Parse target names from a Makefile. We only need names; execution shells out
/// to `make`, which does the real work.
fn parse_makefile(content: &str) -> Vec<Task> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for line in content.lines() {
        // Recipe bodies are tab-indented; skip them and any indented line.
        if line.starts_with('\t') || line.starts_with(' ') {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some(idx) = line.find(':') else { continue };
        // `VAR := value` and friends are assignments, not targets.
        if line.as_bytes().get(idx + 1) == Some(&b'=') {
            continue;
        }
        let name = line[..idx].trim();
        // Real targets are a single token of ordinary characters; skip special
        // (`.PHONY`), pattern (`%`) and variable-expanded targets.
        if name.is_empty()
            || name.contains(char::is_whitespace)
            || name.starts_with('.')
            || name.contains('%')
            || name.contains('$')
        {
            continue;
        }
        if !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-./".contains(&b))
        {
            continue;
        }
        if seen.insert(name.to_string()) {
            out.push(Task {
                name: name.to_string(),
                detail: "make target".to_string(),
                source: SourceKind::Make,
                run: RunSpec::Exec(vec!["make".to_string(), name.to_string()]),
            });
        }
    }
    out
}

/// Parse recipe names from a justfile. Execution delegates to `just`.
fn parse_justfile(content: &str) -> Vec<Task> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for line in content.lines() {
        // Recipe bodies are indented; only column-0 lines declare recipes.
        if line.is_empty() || line.starts_with([' ', '\t']) {
            continue;
        }
        let trimmed = line.trim_end();
        if trimmed.starts_with('#') {
            continue;
        }
        // A leading `@` marks a quiet recipe.
        let body = trimmed.strip_prefix('@').unwrap_or(trimmed);
        // Skip settings, assignments and module directives.
        if body.starts_with("set ")
            || body.starts_with("export ")
            || body.starts_with("alias ")
            || body.starts_with("import ")
            || body.starts_with("mod ")
        {
            continue;
        }
        let Some(idx) = body.find(':') else { continue };
        // `name := value` is a variable assignment.
        if body.as_bytes().get(idx + 1) == Some(&b'=') {
            continue;
        }
        // The recipe name is the first token before any parameters.
        let head = &body[..idx];
        let Some(name) = head.split_whitespace().next() else {
            continue;
        };
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            continue;
        }
        if seen.insert(name.to_string()) {
            out.push(Task {
                name: name.to_string(),
                detail: "just recipe".to_string(),
                source: SourceKind::Just,
                run: RunSpec::Exec(vec!["just".to_string(), name.to_string()]),
            });
        }
    }
    out
}

/// The conventional cargo commands a Rust project can run. We don't parse
/// Cargo.toml beyond confirming it exists; `nr build` simply maps to
/// `cargo build`.
fn cargo_tasks() -> Vec<Task> {
    const CMDS: &[&str] = &[
        "build", "run", "test", "check", "clippy", "fmt", "bench", "doc",
    ];
    CMDS.iter()
        .map(|c| Task {
            name: c.to_string(),
            detail: format!("cargo {c}"),
            source: SourceKind::Cargo,
            run: RunSpec::Exec(vec!["cargo".to_string(), c.to_string()]),
        })
        .collect()
}

/// Parse task tables out of pyproject.toml: pdm, poetry and taskipy. This is a
/// deliberately tiny TOML reader that understands just enough: section headers
/// and top-of-table `key = value` lines.
///
/// poetry and pdm normally bootstrap a Python interpreter on every `run`, which
/// is exactly the overhead `nr` exists to skip. So where we can express the task
/// as a plain command we emit a `Venv` spec (run it in the project virtualenv
/// directly), keeping the launcher only as a fallback.
fn parse_pyproject(content: &str) -> Vec<Task> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    // pdm: inline `name = "cmd"` scripts run directly in the venv. Sub-table
    // forms (`[tool.pdm.scripts.name]` with `call`/`composite`/`env`) keep their
    // pdm semantics, so those delegate to `pdm run`.
    let pdm_inline: std::collections::HashMap<String, String> =
        table_pairs(content, "tool.pdm.scripts")
            .into_iter()
            .collect();
    for name in table_keys(content, "tool.pdm.scripts") {
        if !seen.insert(name.clone()) {
            continue;
        }
        let fallback = vec!["pdm".to_string(), "run".to_string(), name.clone()];
        match pdm_inline.get(&name) {
            Some(cmd) if !cmd.is_empty() && !cmd.starts_with('{') => out.push(Task {
                detail: truncate(cmd),
                run: RunSpec::Venv {
                    command: cmd.clone(),
                    requires: None,
                    fallback,
                },
                name,
                source: SourceKind::Python,
            }),
            _ => out.push(Task {
                detail: "pdm script".to_string(),
                run: RunSpec::Exec(fallback),
                name,
                source: SourceKind::Python,
            }),
        }
    }
    // poetry: `[tool.poetry.scripts]` entries are console scripts installed into
    // the venv's `bin/`, so when that wrapper exists we exec `<venv>/bin/<name>`
    // directly; otherwise (e.g. the project isn't installed) we delegate.
    for name in table_keys(content, "tool.poetry.scripts") {
        if seen.insert(name.clone()) {
            out.push(Task {
                detail: "poetry script".to_string(),
                run: RunSpec::Venv {
                    command: name.clone(),
                    requires: Some(name.clone()),
                    fallback: vec!["poetry".to_string(), "run".to_string(), name.clone()],
                },
                name,
                source: SourceKind::Python,
            });
        }
    }
    // taskipy: the value is a shell command we can run directly.
    for (name, cmd) in table_pairs(content, "tool.taskipy.tasks") {
        if cmd.is_empty() {
            continue;
        }
        if seen.insert(name.clone()) {
            out.push(Task {
                detail: truncate(&cmd),
                run: RunSpec::Shell(cmd),
                name,
                source: SourceKind::Python,
            });
        }
    }
    out
}

/// Parse `[scripts]` from a pipenv Pipfile (TOML). String scripts run directly
/// in the project virtualenv; the table form keeps pipenv's semantics.
fn parse_pipfile(content: &str) -> Vec<Task> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (name, cmd) in table_pairs(content, "scripts") {
        if !seen.insert(name.clone()) {
            continue;
        }
        let fallback = vec!["pipenv".to_string(), "run".to_string(), name.clone()];
        if cmd.is_empty() || cmd.starts_with('{') {
            out.push(Task {
                detail: "pipenv script".to_string(),
                run: RunSpec::Exec(fallback),
                name,
                source: SourceKind::Pipenv,
            });
        } else {
            out.push(Task {
                detail: truncate(&cmd),
                run: RunSpec::Venv {
                    command: cmd.clone(),
                    requires: None,
                    fallback,
                },
                name,
                source: SourceKind::Pipenv,
            });
        }
    }
    out
}

/// Names defined under a TOML table `prefix`: both inline keys directly under
/// `[prefix]` and the first segment of any `[prefix.NAME...]` sub-table.
fn table_keys(content: &str, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut cur = String::new();
    let sub = format!("{prefix}.");
    for line in content.lines() {
        let t = line.trim();
        if let Some(header) = section_header(t) {
            cur = header.to_string();
            if let Some(rest) = cur.strip_prefix(&sub) {
                let name = unquote(rest.split('.').next().unwrap_or(""));
                if !name.is_empty() && seen.insert(name.clone()) {
                    out.push(name);
                }
            }
            continue;
        }
        if cur == prefix && !t.is_empty() && !t.starts_with('#') {
            if let Some(eq) = t.find('=') {
                let key = unquote(t[..eq].trim());
                if !key.is_empty() && seen.insert(key.clone()) {
                    out.push(key);
                }
            }
        }
    }
    out
}

/// Inline `key = value` pairs directly under `[prefix]` (value un-quoted).
fn table_pairs(content: &str, prefix: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in content.lines() {
        let t = line.trim();
        if let Some(header) = section_header(t) {
            cur = header.to_string();
            continue;
        }
        if cur == prefix && !t.is_empty() && !t.starts_with('#') {
            if let Some(eq) = t.find('=') {
                let key = unquote(t[..eq].trim());
                let val = unquote(t[eq + 1..].trim());
                if !key.is_empty() {
                    out.push((key, val));
                }
            }
        }
    }
    out
}

/// Extract the inside of a `[section]` header line, ignoring `[[array]]` tables.
fn section_header(line: &str) -> Option<&str> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    // `[[x]]` array-of-tables leaves a stray bracket; ignore those.
    if inner.starts_with('[') || inner.ends_with(']') {
        return None;
    }
    Some(inner.trim())
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let s = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(s);
    s.to_string()
}

/// Parse `name: command` process definitions from a Procfile.
fn parse_procfile(content: &str) -> Vec<Task> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some(idx) = line.find(':') else { continue };
        let name = line[..idx].trim();
        let cmd = line[idx + 1..].trim();
        if name.is_empty()
            || cmd.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            continue;
        }
        if seen.insert(name.to_string()) {
            out.push(Task {
                name: name.to_string(),
                detail: truncate(cmd),
                source: SourceKind::Procfile,
                run: RunSpec::Shell(cmd.to_string()),
            });
        }
    }
    out
}

/// Parse task names under the top-level `tasks:` key of a go-task Taskfile.
/// Execution delegates to `task`.
fn parse_taskfile(content: &str) -> Vec<Task> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut in_tasks = false;
    let mut key_indent: Option<usize> = None;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent == 0 {
            // A new top-level key ends the tasks block.
            in_tasks = line.trim_start().starts_with("tasks:");
            key_indent = None;
            continue;
        }
        if !in_tasks {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.starts_with('-') {
            continue;
        }
        // The first indented key sets the level at which task names live.
        let level = *key_indent.get_or_insert(indent);
        if indent != level {
            continue; // nested task fields (cmds:, desc:, …)
        }
        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        let name = trimmed[..colon].trim().trim_matches(['"', '\'']);
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-:".contains(&b))
        {
            continue;
        }
        if seen.insert(name.to_string()) {
            out.push(Task {
                name: name.to_string(),
                detail: "task".to_string(),
                source: SourceKind::Taskfile,
                run: RunSpec::Exec(vec!["task".to_string(), name.to_string()]),
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn names(tasks: &[Task]) -> Vec<&str> {
        tasks.iter().map(|t| t.name.as_str()).collect()
    }

    #[test]
    fn npm_scripts_sorted_with_command_preview() {
        let json = r#"{"scripts":{"build":"tsc","test":"vitest run"}}"#;
        let tasks = parse_npm(json);
        assert_eq!(names(&tasks), vec!["build", "test"]);
        assert_eq!(tasks[0].run, RunSpec::Shell("tsc".to_string()));
        assert_eq!(tasks[1].detail, "vitest run");
    }

    #[test]
    fn npm_missing_scripts_is_empty_not_error() {
        assert!(parse_npm(r#"{"name":"x"}"#).is_empty());
        assert!(parse_npm("not json").is_empty());
    }

    #[test]
    fn makefile_targets_skip_assignments_and_specials() {
        let mk = "\
CC := gcc
.PHONY: build test
build: deps
\tcc -o out
test:
\t./out
%.o: %.c
\tcc -c $<
deploy-prod:
\techo go
";
        let tasks = parse_makefile(mk);
        assert_eq!(names(&tasks), vec!["build", "test", "deploy-prod"]);
        assert_eq!(
            tasks[0].run,
            RunSpec::Exec(vec!["make".to_string(), "build".to_string()])
        );
    }

    #[test]
    fn justfile_recipes_skip_vars_and_settings() {
        let jf = "\
set shell := [\"bash\", \"-c\"]
version := \"1.0\"
# a comment
build:
    cargo build
test target=\"all\":
    cargo test
@quiet:
    echo hi
";
        let tasks = parse_justfile(jf);
        assert_eq!(names(&tasks), vec!["build", "test", "quiet"]);
        assert_eq!(
            tasks[1].run,
            RunSpec::Exec(vec!["just".to_string(), "test".to_string()])
        );
    }

    #[test]
    fn cargo_exposes_conventional_commands() {
        let tasks = cargo_tasks();
        assert!(names(&tasks).contains(&"build"));
        assert!(names(&tasks).contains(&"clippy"));
        let build = tasks.iter().find(|t| t.name == "build").unwrap();
        assert_eq!(
            build.run,
            RunSpec::Exec(vec!["cargo".to_string(), "build".to_string()])
        );
    }

    #[test]
    fn pyproject_pdm_poetry_and_taskipy() {
        let toml = "\
[project]
name = \"demo\"

[tool.pdm.scripts]
lint = \"ruff check .\"

[tool.pdm.scripts.serve]
cmd = \"uvicorn app:main\"

[tool.poetry.scripts]
mycli = \"demo.cli:main\"

[tool.taskipy.tasks]
fmt = \"black .\"
";
        let tasks = parse_pyproject(toml);
        let n = names(&tasks);
        assert!(n.contains(&"lint"), "pdm inline: {n:?}");
        assert!(n.contains(&"serve"), "pdm subtable: {n:?}");
        assert!(n.contains(&"mycli"), "poetry: {n:?}");
        assert!(n.contains(&"fmt"), "taskipy: {n:?}");

        // Inline pdm command -> run in the venv directly, fall back to `pdm run`.
        let lint = tasks.iter().find(|t| t.name == "lint").unwrap();
        assert_eq!(
            lint.run,
            RunSpec::Venv {
                command: "ruff check .".to_string(),
                requires: None,
                fallback: vec!["pdm".to_string(), "run".to_string(), "lint".to_string()],
            }
        );
        // Sub-table pdm script keeps pdm semantics -> delegate.
        let serve = tasks.iter().find(|t| t.name == "serve").unwrap();
        assert_eq!(
            serve.run,
            RunSpec::Exec(vec![
                "pdm".to_string(),
                "run".to_string(),
                "serve".to_string()
            ])
        );
        // poetry console script -> exec `<venv>/bin/mycli`, fall back to poetry.
        let mycli = tasks.iter().find(|t| t.name == "mycli").unwrap();
        assert_eq!(
            mycli.run,
            RunSpec::Venv {
                command: "mycli".to_string(),
                requires: Some("mycli".to_string()),
                fallback: vec!["poetry".to_string(), "run".to_string(), "mycli".to_string()],
            }
        );
        // taskipy stays a plain shell command.
        let fmt = tasks.iter().find(|t| t.name == "fmt").unwrap();
        assert_eq!(fmt.run, RunSpec::Shell("black .".to_string()));
    }

    #[test]
    fn pipfile_scripts() {
        let pipfile = "\
[[source]]
url = \"https://pypi.org/simple\"

[packages]
flask = \"*\"

[scripts]
test = \"pytest -q\"
serve = \"flask run\"
";
        let tasks = parse_pipfile(pipfile);
        assert_eq!(names(&tasks), vec!["test", "serve"]);
        assert_eq!(
            tasks[0].run,
            RunSpec::Venv {
                command: "pytest -q".to_string(),
                requires: None,
                fallback: vec!["pipenv".to_string(), "run".to_string(), "test".to_string()],
            }
        );
        assert_eq!(tasks[0].source, SourceKind::Pipenv);
    }

    #[test]
    fn local_venv_bin_finds_in_project_and_pypackages() {
        // in-project .venv/bin
        let a = std::env::temp_dir().join(format!("nr-venv-a-{}", std::process::id()));
        let _ = fs::remove_dir_all(&a);
        fs::create_dir_all(a.join(".venv/bin")).unwrap();
        assert_eq!(local_venv_bin(&a), Some(a.join(".venv/bin")));
        let _ = fs::remove_dir_all(&a);

        // pdm PEP 582 __pypackages__/<ver>/bin
        let b = std::env::temp_dir().join(format!("nr-venv-b-{}", std::process::id()));
        let _ = fs::remove_dir_all(&b);
        fs::create_dir_all(b.join("__pypackages__/3.11/bin")).unwrap();
        assert_eq!(local_venv_bin(&b), Some(b.join("__pypackages__/3.11/bin")));
        let _ = fs::remove_dir_all(&b);

        // nothing -> None
        let c = std::env::temp_dir().join(format!("nr-venv-c-{}", std::process::id()));
        let _ = fs::remove_dir_all(&c);
        fs::create_dir_all(&c).unwrap();
        assert_eq!(local_venv_bin(&c), None);
        let _ = fs::remove_dir_all(&c);
    }

    #[test]
    fn procfile_processes() {
        let pf = "\
# comment
web: bundle exec puma -C config/puma.rb
worker: bundle exec sidekiq
";
        let tasks = parse_procfile(pf);
        assert_eq!(names(&tasks), vec!["web", "worker"]);
        assert_eq!(
            tasks[0].run,
            RunSpec::Shell("bundle exec puma -C config/puma.rb".to_string())
        );
    }

    #[test]
    fn taskfile_top_level_tasks_only() {
        let tf = "\
version: '3'

vars:
  GREETING: Hello

tasks:
  build:
    cmds:
      - go build
  test:
    desc: run tests
    cmds:
      - go test ./...
  lint:
    cmd: golangci-lint run
";
        let tasks = parse_taskfile(tf);
        assert_eq!(names(&tasks), vec!["build", "test", "lint"]);
        assert_eq!(
            tasks[0].run,
            RunSpec::Exec(vec!["task".to_string(), "build".to_string()])
        );
    }

    #[test]
    fn detect_merges_sources_in_one_dir() {
        let dir = std::env::temp_dir().join(format!("nr-detect-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"start":"node ."}}"#,
        )
        .unwrap();
        fs::write(dir.join("Makefile"), "deploy:\n\techo go\n").unwrap();

        let detection = detect_from(&dir).unwrap();
        assert!(detection.manifests.contains(&SourceKind::Npm));
        assert!(detection.manifests.contains(&SourceKind::Make));
        assert!(detection.find("start").is_some());
        assert!(detection.find("deploy").is_some());
        assert_eq!(detection.find("start").unwrap().source, SourceKind::Npm);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn npm_wins_name_conflict() {
        let dir = std::env::temp_dir().join(format!("nr-conflict-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("package.json"), r#"{"scripts":{"test":"vitest"}}"#).unwrap();
        fs::write(dir.join("Makefile"), "test:\n\t./run\n").unwrap();

        let detection = detect_from(&dir).unwrap();
        // Both are listed, but the npm one resolves first.
        assert_eq!(detection.find("test").unwrap().source, SourceKind::Npm);

        let _ = fs::remove_dir_all(&dir);
    }
}
