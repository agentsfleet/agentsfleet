import { resolve } from "node:path";
import { WORKTREE_ROOT, fileHash, outputHash, sourceIdentity, sourceRevision, buildMetadata } from "./build-source";
import { BUILD_RECORD_MAX_BYTES, buildManifestSchema } from "./build-manifest";
const BUILD_INFO_MAX_BYTES = 1024;

const manifestFile = Bun.file(Bun.env.ACCEPTANCE_BUILD_MANIFEST ?? resolve(WORKTREE_ROOT, ".tmp/acceptance-build.json"));
if (manifestFile.size > BUILD_RECORD_MAX_BYTES) throw new Error("Acceptance build manifest exceeds its bound");
const manifest = buildManifestSchema.parse(await manifestFile.json());
const cliFile = resolve(WORKTREE_ROOT, "cli/dist/bin/agentsfleet.js");
const cli = Bun.spawnSync(["bun", cliFile, "--build-info"], { timeout: 10_000 });
if (cli.exitCode !== 0 || cli.stdout.length > BUILD_INFO_MAX_BYTES) throw new Error("Cannot observe CLI build identity");
console.log(JSON.stringify({
  manifest,
  revision: sourceRevision(),
  source_id: await sourceIdentity(),
  metadata: await buildMetadata(),
  daemon_hash: await fileHash(Bun.file(manifest.daemon_path)),
  runner_hash: await fileHash(Bun.file(manifest.runner_path)),
  cli_hash: await outputHash(resolve(WORKTREE_ROOT, "cli/dist")),
  app_hash: await outputHash(resolve(WORKTREE_ROOT, "ui/packages/app/.next")),
  cli: JSON.parse(cli.stdout.toString()),
}));
