import { resolve } from "node:path";
import { WORKTREE_ROOT, fileHash, outputHash, sourceIdentity, sourceRevision, buildMetadata } from "./build-source.ts";
import { BUILD_RECORD_MAX_BYTES, buildManifestSchema } from "./build-manifest.ts";
const BUILD_INFO_MAX_BYTES = 1024;

if (import.meta.main) process.stdout.write(`${JSON.stringify(await inspectAcceptanceBuild())}\n`);

export async function inspectAcceptanceBuild(root = WORKTREE_ROOT) {
  const manifestFile = Bun.file(Bun.env.ACCEPTANCE_BUILD_MANIFEST ?? resolve(root, ".tmp/acceptance-build.json"));
  if (manifestFile.size > BUILD_RECORD_MAX_BYTES) throw new Error("Acceptance build manifest exceeds its bound");
  const manifest = buildManifestSchema.parse(await manifestFile.json());
  const cliFile = resolve(root, "cli/dist/bin/agentsfleet.js");
  const cli = Bun.spawnSync(["bun", cliFile, "--build-info"], {timeout: 10_000});
  if (cli.exitCode !== 0 || cli.stdout.length > BUILD_INFO_MAX_BYTES) throw new Error("Cannot observe CLI build identity");
  return {
    manifest, revision: sourceRevision(root), source_id: await sourceIdentity(root), metadata: await buildMetadata(root),
    daemon_hash: await fileHash(Bun.file(resolve(root, manifest.daemon_path))),
    runner_hash: await fileHash(Bun.file(resolve(root, manifest.runner_path))),
    cli_hash: await outputHash(resolve(root, "cli/dist")), app_hash: await outputHash(resolve(root, "ui/packages/app/.next")),
    cli: JSON.parse(cli.stdout.toString()),
  };
}
