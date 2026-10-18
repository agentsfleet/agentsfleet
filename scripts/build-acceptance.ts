import { resolve } from "node:path";
import { z } from "zod";
import { WORKTREE_ROOT, fileHash, outputHash, sourceIdentity, sourceRevision, buildMetadata } from "./build-source.ts";
import { ACCEPTANCE_ARTIFACTS, readAcceptanceComponents } from "./acceptance-artifacts.ts";
import { buildManifestSchema } from "./build-manifest.ts";

const BUILD_TIMEOUT_MS = 30 * 60_000;
const CARGO_RECORD_MAX_CHARS = 1024 * 1024;
const CARGO_RECORD_TOO_LARGE = "Cargo record exceeds its bound";
const RECORD_SEPARATOR = "\n";
const ARTIFACT = "compiler-artifact";
const messageSchema = z.object({ reason: z.string() });
const artifactSchema = z.object({
  target: z.object({ name: z.string() }), executable: z.string().nullable(),
});
const MANIFEST = ".tmp/acceptance-build.json";
const BUILD = "build";
const PACKAGE_BUILD_COMMAND = ["bun", "run", BUILD];
const STDIO_INHERIT = "inherit";
const STDIO_IGNORE = "ignore";

if (import.meta.main) await runAcceptanceBuild(Bun.argv[2]);

export async function runAcceptanceBuild(mode?: string, root = WORKTREE_ROOT): Promise<void> {
  if (mode === "--assemble") await assemble(root);
  else await buildCandidate(root);
}

async function assemble(root: string): Promise<void> {
  const {daemon, runner, cli, app} = await readAcceptanceComponents(root);
  const manifest = buildManifestSchema.parse({
    source_id: daemon.source_id, revision: daemon.revision,
    revisions: {daemon: daemon.revision, runner: runner.revision, cli: cli.revision, app: app.revision},
    built_at: new Date().toISOString(), environment: Bun.env.ACCEPTANCE_ENVIRONMENT ?? "dev",
    daemon_path: ACCEPTANCE_ARTIFACTS.daemon, runner_path: ACCEPTANCE_ARTIFACTS.runner,
    daemon_hash: daemon.hash, runner_hash: runner.hash, cli_hash: cli.hash, app_hash: app.hash,
    metadata: await buildMetadata(root),
  });
  await Bun.write(resolve(root, MANIFEST), `${JSON.stringify(manifest, null, 2)}\n`);
}

async function build(command: string[], cwd: string, buildId: string, root: string): Promise<void> {
  const child = Bun.spawn(command, {
    cwd: resolve(root, cwd),
    env: { ...Bun.env, AGENTSFLEET_BUILD_ID: buildId },
    stdin: STDIO_IGNORE, stdout: STDIO_INHERIT, stderr: STDIO_INHERIT,
    signal: AbortSignal.timeout(BUILD_TIMEOUT_MS),
  });
  if (await child.exited !== 0) throw new Error(`Acceptance build failed: ${command[0]}`);
}

async function buildCandidate(root: string): Promise<void> {
  const sourceId = await sourceIdentity(root);
  const revision = sourceRevision(root);
  const daemonPath = await buildNative("agentsfleetd", sourceId, root);
  const daemonRevision = checkedRevision();
  const runnerPath = await buildNative("agentsfleet-runner", sourceId, root, Bun.env.ACCEPTANCE_RUNNER_TARGET);
  const runnerRevision = checkedRevision();
  await build(PACKAGE_BUILD_COMMAND, "cli", sourceId, root);
  const cliRevision = checkedRevision();
  await build(PACKAGE_BUILD_COMMAND, "ui/packages/app", sourceId, root);
  const appRevision = checkedRevision();
  if (await sourceIdentity(root) !== sourceId) throw new Error("Sources changed during build; rebuild the candidate");
  const manifest = buildManifestSchema.parse({
    source_id: sourceId, revision,
    revisions: {daemon: daemonRevision, runner: runnerRevision, cli: cliRevision, app: appRevision},
    built_at: new Date().toISOString(), environment: Bun.env.ACCEPTANCE_ENVIRONMENT ?? "local",
    daemon_path: daemonPath, runner_path: runnerPath, metadata: await buildMetadata(root),
    daemon_hash: await fileHash(Bun.file(daemonPath)),
    runner_hash: await fileHash(Bun.file(runnerPath)),
    cli_hash: await outputHash(resolve(root, "cli/dist")),
    app_hash: await outputHash(resolve(root, "ui/packages/app/.next")),
  });
  await Bun.write(resolve(root, MANIFEST), `${JSON.stringify(manifest, null, 2)}\n`);
  process.stdout.write(`Acceptance builds bound to ${sourceId}; manifest: ${resolve(root, MANIFEST)}\n`);

  function checkedRevision(): string {
    const observed = sourceRevision(root);
    if (observed !== revision) throw new Error("Revision changed during build; rebuild the candidate");
    return observed;
  }
}

/** Cargo reports the actual executable, including target-dir, config and cross-target choices. */
async function buildNative(binary: string, buildId: string, root: string, target?: string): Promise<string> {
  const command = ["cargo", BUILD, "--bin", binary, "--message-format=json"];
  if (target) command.push("--target", target);
  const child = Bun.spawn(command, {
    cwd: resolve(root, "rustd"), env: { ...Bun.env, AGENTSFLEET_BUILD_ID: buildId },
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
