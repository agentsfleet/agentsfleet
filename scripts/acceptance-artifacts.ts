import { resolve } from "node:path";
import { z as schema } from "zod";
import { BUILD_COMPONENTS, BUILD_RECORD_MAX_BYTES, buildDigest } from "./build-manifest.ts";
import { WORKTREE_ROOT, fileHash, outputHash, sourceIdentity, sourceRevision } from "./build-source.ts";

export const ACCEPTANCE_ARTIFACTS = {
  daemon: "dist/agentsfleetd-rs-linux-amd64",
  runner: "dist/agentsfleet-runner-linux-amd64",
  cli: "cli/dist",
  app: "ui/packages/app/.next",
} as const;
export const componentSchema = schema.enum(BUILD_COMPONENTS);
export type BuildComponent = schema.infer<typeof componentSchema>;
const RECEIPTS = "dist/acceptance-components";
const BUILD_TIMEOUT_MS = 30 * 60_000;
const STDIO_INHERIT = "inherit";
const receiptSchema = schema.object({
  component: componentSchema, source_id: buildDigest,
  revision: schema.string().regex(/^[0-9a-f]{40}$/), hash: buildDigest,
});
type BuildReceipt = schema.infer<typeof receiptSchema>;
type BuildOptions = {root?: string; timeoutMs?: number};

/** The receipt is written only after a successful build with unchanged sources. */
export async function buildAcceptanceComponent(component: BuildComponent, command: string[], options: BuildOptions = {}): Promise<BuildReceipt> {
  if (!command.length) throw new Error("A component build command is required");
  const root = options.root ?? WORKTREE_ROOT;
  const record = Bun.file(resolve(root, RECEIPTS, `${component}.json`));
  if (await record.exists()) await record.delete();
  const revision = sourceRevision(root);
  const sourceId = await sourceIdentity(root);
  const child = Bun.spawn(command, {
    cwd: root, env: {...Bun.env, AGENTSFLEET_BUILD_ID: sourceId},
    stdin: STDIO_INHERIT, stdout: STDIO_INHERIT, stderr: STDIO_INHERIT,
    signal: AbortSignal.timeout(options.timeoutMs ?? BUILD_TIMEOUT_MS),
  });
  try {
    if (await child.exited !== 0) throw new Error(`Acceptance component build failed: ${component}`);
  } finally {
    if (child.exitCode === null) child.kill();
    await child.exited;
  }
  if (sourceRevision(root) !== revision || await sourceIdentity(root) !== sourceId) {
    throw new Error("Sources or revision changed during the component build");
  }
  const receipt = receiptSchema.parse({component, source_id: sourceId, revision, hash: await artifactHash(component, root)});
  await Bun.write(record, `${JSON.stringify(receipt)}\n`);
  return receipt;
}

/** Compose the four deployment builds without rebuilding any of them. */
export async function readAcceptanceComponents(root = WORKTREE_ROOT): Promise<Record<BuildComponent, BuildReceipt>> {
  const revision = sourceRevision(root);
  const sourceId = await sourceIdentity(root);
  // The component tuple fixes this fan-out at four local reads.
  const receipts = await Promise.all(BUILD_COMPONENTS.map(async (component) => {
    const file = Bun.file(resolve(root, RECEIPTS, `${component}.json`));
    if (file.size > BUILD_RECORD_MAX_BYTES) throw new Error("Component build receipt exceeds its bound");
    const receipt = receiptSchema.parse(await file.json());
    if (receipt.component !== component || receipt.revision !== revision || receipt.source_id !== sourceId) {
      throw new Error(`Acceptance ${component} receipt differs from the candidate`);
    }
    if (receipt.hash !== await artifactHash(component, root)) throw new Error(`Acceptance ${component} artifact differs from its build receipt`);
    return receipt;
  }));
  const [daemon, runner, cli, app] = receipts;
  if (!daemon || !runner || !cli || !app) throw new Error("Acceptance components are incomplete");
  return {daemon, runner, cli, app};
}

async function artifactHash(component: BuildComponent, root: string): Promise<string> {
  const path = resolve(root, ACCEPTANCE_ARTIFACTS[component]);
  return component === BUILD_COMPONENTS[0] || component === BUILD_COMPONENTS[1]
    ? await fileHash(Bun.file(path)) : await outputHash(path);
}
