#!/usr/bin/env node

const { execSync, spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");
const os = require("os");

const ROOT = path.join(__dirname, "..");
const README = path.join(ROOT, "README.md");
const RUNS = 10;
const WARMUP = 3;

// Detect system info
function getSystemInfo() {
  const platform = os.platform();
  const arch = os.arch();

  if (platform === "darwin") {
    const version = execSync("sw_vers -productVersion", {
      encoding: "utf8",
    }).trim();
    const chip = arch === "arm64" ? "Apple Silicon" : "Intel";
    return `macOS ${version} (${chip})`;
  } else if (platform === "linux") {
    try {
      const release = fs.readFileSync("/etc/os-release", "utf8");
      const name = release.match(/^NAME="?([^"\n]+)"?/m)?.[1] || "Linux";
      const version = release.match(/^VERSION_ID="?([^"\n]+)"?/m)?.[1] || "";
      return `${name} ${version}`.trim();
    } catch {
      return `Linux ${os.release()}`;
    }
  } else if (platform === "win32") {
    return `Windows ${os.release()}`;
  }
  return `${platform} ${arch}`;
}

// Check if command exists
function hasCommand(cmd) {
  try {
    const which = process.platform === "win32" ? "where" : "which";
    execSync(`${which} ${cmd}`, { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

// Get install size for a runner
function getInstallSize(runner) {
  try {
    if (runner === "nr") {
      const nrPath = path.join(ROOT, "target", "release", process.platform === "win32" ? "nr.exe" : "nr");
      if (fs.existsSync(nrPath)) {
        const stats = fs.statSync(nrPath);
        return stats.size;
      }
    } else if (runner === "bun") {
      const bunPath = execSync("which bun", { encoding: "utf8" }).trim();
      const stats = fs.statSync(bunPath);
      return stats.size;
    } else if (runner === "npm") {
      const npmPath = execSync("which npm", { encoding: "utf8" }).trim();
      const npmDir = path.join(path.dirname(npmPath), "..", "lib", "node_modules", "npm");
      const size = execSync(`du -sb "${npmDir}" 2>/dev/null || du -sk "${npmDir}" | awk '{print $1 * 1024}'`, { encoding: "utf8" }).trim();
      return parseInt(size.split(/\s+/)[0], 10);
    } else if (runner === "yarn") {
      const yarnPath = execSync("which yarn", { encoding: "utf8" }).trim();
      const yarnDir = path.join(path.dirname(yarnPath), "..", "lib", "node_modules", "yarn");
      const size = execSync(`du -sb "${yarnDir}" 2>/dev/null || du -sk "${yarnDir}" | awk '{print $1 * 1024}'`, { encoding: "utf8" }).trim();
      return parseInt(size.split(/\s+/)[0], 10);
    } else if (runner === "pnpm") {
      const pnpmPath = execSync("which pnpm", { encoding: "utf8" }).trim();
      const realPath = execSync(`realpath "${pnpmPath}"`, { encoding: "utf8" }).trim();
      const pnpmDir = path.join(path.dirname(realPath), "..");
      const size = execSync(`du -sb "${pnpmDir}" 2>/dev/null || du -sk "${pnpmDir}" | awk '{print $1 * 1024}'`, { encoding: "utf8" }).trim();
      return parseInt(size.split(/\s+/)[0], 10);
    }
  } catch {
    return null;
  }
  return null;
}

// Format bytes to human readable
function formatSize(bytes) {
  if (bytes === null) return "N/A";
  if (bytes < 1024 * 1024) {
    return `${Math.round(bytes / 1024)}KB`;
  }
  return `${Math.round(bytes / (1024 * 1024))}MB`;
}

// Format milliseconds, with a decimal when sub-10ms differences matter
function formatTime(ms) {
  return ms < 10 ? `${ms.toFixed(1)}ms` : `${Math.round(ms)}ms`;
}

// Benchmark all commands in one hyperfine run (no shell wrapper around
// the measured commands, so fast runners aren't drowned in shell startup)
function benchmarkAll(commands) {
  const jsonPath = path.join(os.tmpdir(), `nr-benchmark-${process.pid}.json`);
  const result = spawnSync(
    "hyperfine",
    [
      "--shell=none",
      "--warmup", String(WARMUP),
      "--runs", String(RUNS),
      "--ignore-failure",
      "--export-json", jsonPath,
      ...commands,
    ],
    { cwd: ROOT, stdio: "inherit" },
  );
  if (result.status !== 0) {
    console.error("hyperfine failed");
    process.exit(result.status ?? 1);
  }
  const data = JSON.parse(fs.readFileSync(jsonPath, "utf8"));
  fs.unlinkSync(jsonPath);
  return data.results.map((r) => r.median * 1000);
}

if (!hasCommand("hyperfine")) {
  console.error(
    "hyperfine is required. Install it with: brew install hyperfine\n" +
      "https://github.com/sharkdp/hyperfine",
  );
  process.exit(1);
}

console.log("Benchmarking nr...");
console.log(`System: ${getSystemInfo()}`);
console.log(`Runs per runner: ${RUNS} (after ${WARMUP} warmup runs)\n`);

// Ensure nr is built
const nrBinary = path.join(
  ROOT,
  "target",
  "release",
  process.platform === "win32" ? "nr.exe" : "nr",
);
if (!fs.existsSync(nrBinary)) {
  console.log("Building nr...");
  execSync("cargo build --release", { cwd: ROOT, stdio: "inherit" });
}

// Collect available runners
const runners = [{ runner: "nr", cmd: `${nrBinary} test` }];

if (hasCommand("npm")) {
  runners.push({ runner: "npm", cmd: "npm run test --silent" });
}

const nodeVersion = parseInt(process.version.slice(1).split(".")[0], 10);
if (nodeVersion >= 22) {
  runners.push({ runner: "node --run", cmd: "node --run test" });
}

if (hasCommand("bun")) {
  runners.push({ runner: "bun", cmd: "bun run --silent test" });
}

if (hasCommand("yarn")) {
  runners.push({ runner: "yarn", cmd: "yarn --silent test" });
}

if (hasCommand("pnpm")) {
  runners.push({ runner: "pnpm", cmd: "pnpm run --silent test" });
}

// Run benchmarks
const times = benchmarkAll(runners.map((r) => r.cmd));
const results = runners.map(({ runner }, i) => ({
  runner,
  time: times[i],
  size: runner === "node --run" ? null : getInstallSize(runner),
}));

// Sort by time and calculate speedups
results.sort((a, b) => a.time - b.time);
const baseline = Math.max(...results.map((r) => r.time));

// Calculate nr's speedup (the fastest)
const nrResult = results.find((r) => r.runner === "nr");
const fastestSpeedup = nrResult ? Math.round(baseline / nrResult.time) : 1;

// Generate table
const tableRows = results.map(({ runner, time, size }) => {
  const speedup = (baseline / time).toFixed(1);
  const speedupStr = runner === "nr" ? `**${speedup}x**` : `${speedup}x`;
  return `| ${runner} | ${formatTime(time)} | ${speedupStr} | ${formatSize(size)} |`;
});

const benchmarkContent = `<!-- BENCHMARK_START -->
| Runner | Time | Speedup | Size |
|--------|------|---------|------|
${tableRows.join("\n")}

*Median of ${RUNS} runs measured with [hyperfine](https://github.com/sharkdp/hyperfine) (\`--shell=none\`) running \`echo test\` on ${getSystemInfo()}. Your mileage may vary.*
<!-- BENCHMARK_END -->`;

// Update README
let readme = fs.readFileSync(README, "utf8");
const startMarker = "<!-- BENCHMARK_START -->";
const endMarker = "<!-- BENCHMARK_END -->";

const startIdx = readme.indexOf(startMarker);
const endIdx = readme.indexOf(endMarker) + endMarker.length;

if (startIdx === -1 || endIdx === -1) {
  console.error("Could not find benchmark markers in README.md");
  process.exit(1);
}

readme = readme.slice(0, startIdx) + benchmarkContent + readme.slice(endIdx);

// Update fastest speedup marker
const speedupStart = "<!-- FASTEST_SPEEDUP_START -->";
const speedupEnd = "<!-- FASTEST_SPEEDUP_END -->";
const speedupStartIdx = readme.indexOf(speedupStart);
const speedupEndIdx = readme.indexOf(speedupEnd);

if (speedupStartIdx !== -1 && speedupEndIdx !== -1) {
  readme =
    readme.slice(0, speedupStartIdx) +
    `${speedupStart}${fastestSpeedup}${speedupEnd}` +
    readme.slice(speedupEndIdx + speedupEnd.length);
}

fs.writeFileSync(README, readme);

console.log(
  `Updated ${README} with benchmark results (${fastestSpeedup}x speedup)`,
);
