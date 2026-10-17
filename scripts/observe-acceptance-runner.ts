import { hostname } from "node:os";
import { resolve } from "node:path";
import { z } from "zod";
import { BUILD_RECORD_MAX_BYTES, buildManifestSchema, runnerProcessSchema } from "./build-manifest.ts";
import { WORKTREE_ROOT, fileHash } from "./build-source.ts";

const ELF_MAGIC = new Uint8Array([0x7f, 0x45, 0x4c, 0x46]);
const argumentsSchema = z.tuple([z.coerce.number().int().positive(), z.uuid(), z.string().min(1).max(4096)]);

/** Run on the Linux host: hash the executable of a live process, never a copied evidence template. */
export async function observeRunner(args = Bun.argv.slice(2), root = WORKTREE_ROOT): Promise<void> {
  if (process.platform !== "linux") throw new Error("Observe the runner on its actual Linux host");
  const [pid, runnerId, destination] = argumentsSchema.parse(args);
  const manifestFile = Bun.file(Bun.env.ACCEPTANCE_BUILD_MANIFEST ?? resolve(root, ".tmp/acceptance-build.json"));
  if (manifestFile.size > BUILD_RECORD_MAX_BYTES) throw new Error("Acceptance build manifest exceeds its bound");
  const manifest = buildManifestSchema.parse(await manifestFile.json());
  process.kill(pid, 0);
  const executable = Bun.file(`/proc/${pid}/exe`);
  const magic = new Uint8Array(await executable.slice(0, ELF_MAGIC.length).arrayBuffer());
  if (!ELF_MAGIC.every((byte, index) => magic[index] === byte)) throw new Error("Runner process executable is not a Linux binary");
  const binaryHash = await fileHash(executable);
  process.kill(pid, 0);
  if (binaryHash !== manifest.runner_hash) throw new Error("Live runner executable differs from the candidate build");
  const evidence = runnerProcessSchema.parse({
    runner_id: runnerId, source_id: manifest.source_id, binary_hash: binaryHash,
    pid, host: hostname(), observed_at: new Date().toISOString(), os: process.platform, arch: process.arch,
  });
  await Bun.write(destination, `${JSON.stringify(evidence, null, 2)}\n`);
  console.log(`Observed candidate runner ${runnerId}, process ${pid}; record: ${destination}`);
}

if (import.meta.main) await observeRunner();
