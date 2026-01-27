#!/usr/bin/env node

const { execSync, spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");
const os = require("os");

const ROOT = path.join(__dirname, "..");
const README = path.join(ROOT, "README.md");
const ITERATIONS = 10;

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

// Benchmark a command
function benchmark(cmd) {
  const times = [];

  for (let i = 0; i < ITERATIONS; i++) {
    const start = performance.now();
    try {
      execSync(cmd, { stdio: "ignore", cwd: ROOT });
    } catch {}
    const end = performance.now();
    times.push(end - start);
  }

  // Return average, excluding outliers
  times.sort((a, b) => a - b);
  const trimmed = times.slice(1, -1); // Remove fastest and slowest
  const avg =
    trimmed.length > 0
      ? trimmed.reduce((a, b) => a + b, 0) / trimmed.length
      : times.reduce((a, b) => a + b, 0) / times.length;

  return Math.round(avg);
}

console.log("Benchmarking nr...");
console.log(`System: ${getSystemInfo()}`);
console.log(`Iterations per runner: ${ITERATIONS}\n`);

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

// Run benchmarks
const results = [];

// nr
process.stdout.write("  nr: ");
const nrTime = benchmark(`"${nrBinary}" test`);
console.log(`${nrTime}ms`);
results.push({ runner: "nr", time: nrTime });

// npm
if (hasCommand("npm")) {
  process.stdout.write("  npm: ");
  const npmTime = benchmark("npm run test --silent");
  console.log(`${npmTime}ms`);
  results.push({ runner: "npm", time: npmTime });
}

// bun
if (hasCommand("bun")) {
  process.stdout.write("  bun: ");
  const bunTime = benchmark("bun run --silent test");
  console.log(`${bunTime}ms`);
  results.push({ runner: "bun", time: bunTime });
}

// yarn
if (hasCommand("yarn")) {
  process.stdout.write("  yarn: ");
  const yarnTime = benchmark("yarn --silent test");
  console.log(`${yarnTime}ms`);
  results.push({ runner: "yarn", time: yarnTime });
}

// pnpm
if (hasCommand("pnpm")) {
  process.stdout.write("  pnpm: ");
  const pnpmTime = benchmark("pnpm run --silent test");
  console.log(`${pnpmTime}ms`);
  results.push({ runner: "pnpm", time: pnpmTime });
}

console.log("");

// Sort by time and calculate speedups
results.sort((a, b) => a.time - b.time);
const baseline = Math.max(...results.map((r) => r.time));

// Calculate nr's speedup (the fastest)
const nrResult = results.find((r) => r.runner === "nr");
const fastestSpeedup = nrResult ? Math.round(baseline / nrResult.time) : 1;

// Generate table
const tableRows = results.map(({ runner, time }) => {
  const speedup = (baseline / time).toFixed(1);
  const speedupStr = runner === "nr" ? `**${speedup}x**` : `${speedup}x`;
  return `| ${runner} | ${time}ms | ${speedupStr} |`;
});

const benchmarkContent = `<!-- BENCHMARK_START -->
| Runner | Time | Speedup |
|--------|------|---------|
${tableRows.join("\n")}

*Measured running \`echo test\` on ${getSystemInfo()}. Your mileage may vary.*
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
