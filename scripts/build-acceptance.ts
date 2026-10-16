import { resolve } from "node:path";
import { z } from "zod";
import { WORKTREE_ROOT, fileHash, outputHash, sourceIdentity, sourceRevision, buildMetadata } from "./build-source";
import { buildManifestSchema } from "./build-manifest";

const BUILD_TIMEOUT_MS = 30 * 60_000;
const CARGO_RECORD_MAX_CHARS = 1024 * 1024;
const CARGO_RECORD_TOO_LARGE = "Cargo record exceeds its bound";
const RECORD_SEPARATOR = "\n";
const ARTIFACT = "compiler-artifact";
const messageSchema = z.object({ reason: z.string() });
const artifactSchema = z.object({
  target: z.object({ name: z.string() }), executable: z.string().nullable(),
});
const MANIFEST = resolve(WORKTREE_ROOT, ".tmp/acceptance-build.json");
const BUILD = "build";
const PACKAGE_BUILD_COMMAND = ["bun", "run", BUILD];
const STDIO_INHERIT = "inherit";
const STDIO_IGNORE = "ignore";

async function build(command: string[], cwd: string, buildId: string): Promise<void> {
  const child = Bun.spawn(command, {
    cwd: resolve(WORKTREE_ROOT, cwd),
    env: { ...Bun.env, AGENTSFLEET_BUILD_ID: buildId },
    stdin: STDIO_IGNORE, stdout: STDIO_INHERIT, stderr: STDIO_INHERIT,
    signal: AbortSignal.timeout(BUILD_TIMEOUT_MS),
  });
  if (await child.exited !== 0) throw new Error(`Acceptance build failed: ${command[0]}`);
}

const sourceId = await sourceIdentity();
const revision = sourceRevision();
const daemonPath = await buildNative("agentsfleetd", sourceId);
const daemonRevision = checkedRevision();
const runnerPath = await buildNative("agentsfleet-runner", sourceId, Bun.env.ACCEPTANCE_RUNNER_TARGET);
const runnerRevision = checkedRevision();
await build(PACKAGE_BUILD_COMMAND, "cli", sourceId);
const cliRevision = checkedRevision();
await build(PACKAGE_BUILD_COMMAND, "ui/packages/app", sourceId);
const appRevision = checkedRevision();
if (await sourceIdentity() !== sourceId) throw new Error("Sources changed during build; rebuild the candidate");
const manifest = buildManifestSchema.parse({
  source_id: sourceId, revision,
  revisions: {daemon: daemonRevision, runner: runnerRevision, cli: cliRevision, app: appRevision},
  built_at: new Date().toISOString(), environment: Bun.env.ACCEPTANCE_ENVIRONMENT ?? "local",
  daemon_path: daemonPath, runner_path: runnerPath, metadata: await buildMetadata(),
  daemon_hash: await fileHash(Bun.file(daemonPath)),
  runner_hash: await fileHash(Bun.file(runnerPath)),
  cli_hash: await outputHash(resolve(WORKTREE_ROOT, "cli/dist")),
  app_hash: await outputHash(resolve(WORKTREE_ROOT, "ui/packages/app/.next")),
});
await Bun.write(MANIFEST, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`Acceptance builds bound to ${sourceId}; manifest: ${MANIFEST}`);

function checkedRevision(): string {
  const observed = sourceRevision();
  if (observed !== revision) throw new Error("Revision changed during build; rebuild the candidate");
  return observed;
}

/** Cargo reports the actual executable, including target-dir, config and cross-target choices. */
async function buildNative(binary: string, buildId: string, target?: string): Promise<string> {
  const command = ["cargo", BUILD, "--bin", binary, "--message-format=json"];
  if (target) command.push("--target", target);
  const child = Bun.spawn(command, {
    cwd: resolve(WORKTREE_ROOT, "rustd"), env: { ...Bun.env, AGENTSFLEET_BUILD_ID: buildId },
    stdin: STDIO_IGNORE, stdout: "pipe", stderr: STDIO_INHERIT, signal: AbortSignal.timeout(BUILD_TIMEOUT_MS),
  });
  let executable: string | undefined;
  try {
    for await (const line of cargoRecords(child.stdout)) {
      const raw: unknown = JSON.parse(line);
      if (messageSchema.parse(raw).reason !== ARTIFACT) continue;
      const artifact = artifactSchema.parse(raw);
      if (artifact.target.name === binary && artifact.executable) executable = artifact.executable;
    }
    if (await child.exited !== 0) throw new Error(`Acceptance native build failed: ${binary}`);
    if (!executable) throw new Error(`Cargo reported no executable for ${binary}`);
    return executable;
  } finally {
    if (child.exitCode === null) child.kill();
    await child.exited;
  }
}

async function* cargoRecords(stream: ReadableStream<Uint8Array>): AsyncGenerator<string> {
  const reader = stream.getReader();
  const decoder = new TextDecoder("utf-8", { fatal: true });
  let pending = "";
  try {
    for (;;) {
      const next = await reader.read();
      pending += next.done ? decoder.decode() : decoder.decode(next.value, { stream: true });
      let boundary = pending.indexOf(RECORD_SEPARATOR);
      while (boundary !== -1) {
        if (boundary > CARGO_RECORD_MAX_CHARS) throw new Error(CARGO_RECORD_TOO_LARGE);
        if (boundary > 0) yield pending.slice(0, boundary);
        pending = pending.slice(boundary + 1);
        boundary = pending.indexOf(RECORD_SEPARATOR);
      }
      if (pending.length > CARGO_RECORD_MAX_CHARS) throw new Error(CARGO_RECORD_TOO_LARGE);
      if (next.done) break;
    }
    if (pending) yield pending;
  } finally { await reader.cancel(); reader.releaseLock(); }
}
