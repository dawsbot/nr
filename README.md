<p align="center">
  <img src="logo.svg" alt="nr logo" width="200">
</p>

# nr

Run npm scripts. <!-- FASTEST_SPEEDUP_START -->26<!-- FASTEST_SPEEDUP_END -->x faster.

A zero-overhead npm script runner written in Rust. No Node.js startup, no npm overhead—just your script.

## Benchmarks

<!-- BENCHMARK_START -->
| Runner | Time | Speedup | Size |
|--------|------|---------|------|
| nr | 10ms | **25.6x** | 377KB |
| bun | 11ms | 23.3x | 55MB |
| npm | 125ms | 2.0x | 18MB |
| yarn | 133ms | 1.9x | 5MB |
| pnpm | 256ms | 1.0x | 19MB |

*Measured running `echo test` on macOS 26.2 (Apple Silicon). Your mileage may vary.*
<!-- BENCHMARK_END -->

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/dawsbot/nr/main/install.sh | sh
```

Works on macOS, Linux, and Windows (via Git Bash/WSL).

## Usage

```bash
# List available scripts
nr

# Run a script
nr build

# Pass arguments to the script
nr test -- --watch
```

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
