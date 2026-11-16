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

await mkdir(OUTPUT_DIR, {recursive: true});
await buildNative(DAEMON);
await buildNative(RUNNER);
for (const directory of [CLI_DIR, "ui/packages/app"]) {
  const cwd = resolve(ROOT, directory);
  await rm(resolve(cwd, directory === CLI_DIR ? "dist" : ".next"), {recursive: true, force: true});
  await run(PACKAGE_BUILD, cwd);
}

async function buildNative(binary: string): Promise<void> {
  await run(["cargo", BUILD, "--locked", "--bin", binary, "--target-dir", TARGET_DIR], RUST_ROOT);
  await cp(resolve(TARGET_DIR, "debug", binary), resolve(OUTPUT_DIR, binary));
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
