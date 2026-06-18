<p align="center">
  <img src="logo.svg" alt="nr logo" width="200">
</p>

# nr

Run your project's tasks. <!-- FASTEST_SPEEDUP_START -->66<!-- FASTEST_SPEEDUP_END -->x faster.

A zero-overhead task runner written in Rust. One command for every project. `nr <task>` detects whatever your project uses (npm scripts, a Makefile, a justfile, Cargo, pyproject, uv, a Pipfile, a Procfile or a Taskfile) and runs the task with no Node.js startup and no overhead.

## Benchmarks

<!-- BENCHMARK_START -->
| Runner | Time | Speedup | Size |
|--------|------|---------|------|
| nr | 3.7ms | **66.4x** | 393KB |
| bun | 7.3ms | 34.0x | 60MB |
| node --run | 24ms | 10.2x | N/A |
| npm | 109ms | 2.3x | 18MB |
| yarn | 127ms | 1.9x | 5MB |
| pnpm | 247ms | 1.0x | 19MB |

*Median of 10 runs measured with [hyperfine](https://github.com/sharkdp/hyperfine) (`--shell=none`) running `echo test` on macOS 26.5.1 (Apple Silicon). Your mileage may vary.*
<!-- BENCHMARK_END -->

The table above is `nr` versus other **npm** script runners. `nr` also replaces the **Python** launchers natively: poetry, pipenv and pdm each boot a Python interpreter on every `run`, so `nr` finds the project virtualenv and execs your task directly instead.

| Launcher | Their time | `nr` | Speedup |
|----------|-----------|------|---------|
| `pdm run` | 570ms | 5.1ms | **~110x** |
| `poetry run` | 561ms | 26ms | **~22x** |
| `pipenv run` | 253ms | 5.3ms | **~47x** |

**What about [uv](https://docs.astral.sh/uv/)?** It's already native and fast, so `nr` just delegates to `uv run` (detecting uv projects via `uv.lock`) for one command across every project, not for speed.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/dawsbot/nr/main/install.sh | sh
```

Works on macOS, Linux, and Windows (via Git Bash/WSL).

## Usage

```bash
# List every task nr can find in this project
nr

# Run a task by name
nr build

# Pass arguments through to the task
nr test -- --watch
```

## Task sources

`nr` walks up from your current directory to the nearest folder containing a recognized manifest, then merges every task it finds. If several manifests live side by side, all of their tasks are listed and `package.json` wins any name collision.

| Manifest | Tasks come from | How `nr <task>` runs it |
|----------|-----------------|-------------------------|
| `package.json` | the `scripts` field | direct `exec`, with `node_modules/.bin` on PATH |
| `Makefile` | targets | `make <task>` |
| `justfile` | recipes | `just <task>` |
| `Cargo.toml` | conventional commands (`build`, `test`, `run`, `check`, `clippy`, `fmt`, `bench`, `doc`) | `cargo <task>` |
| `pyproject.toml` | `[tool.pdm.scripts]`, `[tool.poetry.scripts]`, `[tool.taskipy.tasks]` | natively in the project virtualenv (see below) |
| `pyproject.toml` + `uv.lock` | `[project.scripts]` | `uv run <task>` |
| `Pipfile` | `[scripts]` | natively in the project virtualenv (see below) |
| `Procfile` | process names | the process command, via the shell |
| `Taskfile.yml` | the `tasks:` block | `task <task>` |

For delegated tools (`make`, `just`, `cargo`, `task`, `uv`), `nr` replaces itself with a single `exec` call, so it adds no measurable overhead on top of the tool you were going to run anyway. That tool does need to be installed and on your PATH.

For the Python launchers (`poetry`, `pdm`, `pipenv`), `nr` skips the launcher entirely: it resolves the project virtualenv (an active `$VIRTUAL_ENV`, an in-project `.venv`, or pdm's `__pypackages__`), puts its `bin/` on PATH, and execs the task directly, avoiding the Python interpreter startup those tools pay on every `run`. When no local virtualenv can be found it falls back to `poetry run` / `pdm run` / `pipenv run`, so the result is always correct.

## AI Assistant Setup

Make your AI coding assistant use `nr` instead of `npm run` by adding this instruction to its config:

```
Use `nr` instead of `npm run` for running package.json scripts. It's a faster drop-in replacement.
```

### Claude Code

Add to `~/.claude/CLAUDE.md` (global) or `CLAUDE.md` in your project root:

```markdown
Use `nr` instead of `npm run` for running package.json scripts.
```

### Cursor

Add to `.cursorrules` in your project root:

```
Use `nr` instead of `npm run` for running package.json scripts.
```

### GitHub Copilot

Add to `.github/copilot-instructions.md` in your repo:

```markdown
Use `nr` instead of `npm run` for running package.json scripts.
```

### Windsurf

Add to `.windsurfrules` in your project root:

```
Use `nr` instead of `npm run` for running package.json scripts.
```

### Zed

Add to `.zed/settings.json` in your project:

```json
{
  "assistant": {
    "instructions": "Use `nr` instead of `npm run` for running package.json scripts."
  }
}
```

## Why is it faster?

**No Node.js startup.** npm, yarn, and pnpm all bootstrap Node.js before doing anything. That's 50-100ms before your script even starts. `nr` is a native binary—it starts instantly.

**Direct exec.** On Unix systems, `nr` uses the `exec()` syscall to replace itself with your command. No fork, no wait, no overhead. Your script runs in the same process slot.

**No shell when none is needed.** If a script has no shell metacharacters (most don't: `vitest`, `tsc -p .`, `eslint src`), `nr` skips `/bin/sh` entirely and execs the program directly. That saves several milliseconds of shell startup, and your extra arguments arrive as real argv entries instead of being re-split by the shell.

**Minimal parsing.** We only read the `scripts` field from package.json. Not dependencies, not lockfiles, not node_modules. Just scripts.

**Tiny binary.** 377KB. No runtime, no GC, no framework. Just machine code.

## What it doesn't do

`nr` is intentionally minimal:

- No `pre`/`post` lifecycle scripts (use `nr pretest && nr test` if needed)
- No workspaces support

If you need these features, use npm. If you want speed, use `nr`.

## Build from source

```bash
git clone https://github.com/dawsbot/nr
cd nr
cargo build --release
cp target/release/nr /usr/local/bin/
```

## Status

Experimental. Works on my machine. Report issues at [GitHub](https://github.com/dawsbot/nr/issues).

## License

MIT

<!--
## Release

```bash
git add .
git commit -m "Initial release"
git tag v0.1.0
git push origin main --tags
```
-->
