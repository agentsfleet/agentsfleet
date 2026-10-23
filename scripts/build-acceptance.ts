import { cp, mkdir, rm } from "node:fs/promises";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const RUST_ROOT = resolve(ROOT, "rustd");
const OUTPUT_DIR = resolve(ROOT, "dist/acceptance");
const TARGET_DIR = resolve(ROOT, Bun.env.CARGO_TARGET_DIR ?? "rustd/target");
const BUILD_TIMEOUT_MS = 30 * 60_000;
const DAEMON = "agentsfleetd";
const RUNNER = "agentsfleet-runner";
const STDIO_INHERIT = "inherit";
const BUILD = "build";
const CLI_DIR = "cli";
const PACKAGE_BUILD = ["bun", "run", "--bun", BUILD];
const LINUX_BUILD = `set -euo pipefail
cd "$1/rustd"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="$HOME/.cache/agentsfleet/acceptance-target"
cargo build --locked --release --bin agentsfleet-runner
install -m 755 "$CARGO_TARGET_DIR/release/agentsfleet-runner" "$2"
`;

await mkdir(OUTPUT_DIR, {recursive: true});
await buildNative(DAEMON);
if (Bun.env.ACCEPTANCE_RUNNER_MACHINE) {
  await buildLinuxRunner(Bun.env.ACCEPTANCE_RUNNER_MACHINE);
} else {
  await buildNative(RUNNER, Bun.env.ACCEPTANCE_RUNNER_TARGET);
}
for (const directory of [CLI_DIR, "ui/packages/app"]) {
  const cwd = resolve(ROOT, directory);
  await rm(resolve(cwd, directory === CLI_DIR ? "dist" : ".next"), {recursive: true, force: true});
  await run(PACKAGE_BUILD, cwd);
}

async function buildNative(binary: string, target?: string): Promise<void> {
  const command = ["cargo", BUILD, "--locked", "--bin", binary, "--target-dir", TARGET_DIR];
  if (target) command.push("--target", target);
  await run(command, RUST_ROOT);
  await cp(resolve(TARGET_DIR, target ?? "", "debug", binary), resolve(OUTPUT_DIR, binary));
}

async function buildLinuxRunner(machine: string): Promise<void> {
  const child = Bun.spawn(["orb", "-m", machine, "bash", "-s", "--", ROOT, resolve(OUTPUT_DIR, RUNNER)], {
    stdin: "pipe", stdout: STDIO_INHERIT, stderr: STDIO_INHERIT, signal: AbortSignal.timeout(BUILD_TIMEOUT_MS),
  });
  try {
    child.stdin.write(LINUX_BUILD);
    child.stdin.end();
    if (await child.exited !== 0) throw new Error("Acceptance Linux runner build failed");
  } finally {
    if (child.exitCode === null) child.kill();
    await child.exited;
  }
}

async function run(command: string[], cwd: string): Promise<void> {
  const child = Bun.spawn(command, {
    cwd, stdin: "ignore", stdout: STDIO_INHERIT, stderr: STDIO_INHERIT,
    signal: AbortSignal.timeout(BUILD_TIMEOUT_MS),
  });
  try {
    if (await child.exited !== 0) throw new Error(`Acceptance build failed: ${command.join(" ")}`);
  } finally {
    if (child.exitCode === null) child.kill();
    await child.exited;
  }
}
