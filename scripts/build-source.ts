import { resolve } from "node:path";
import { z } from "zod";
import { buildMetadataSchema } from "./build-manifest.ts";

const SOURCE_ROOTS = ["rustd/", "cli/", "ui/", "schema/", "scripts/", "make/"];
const GIT = "git";
const VERSION_FILE = "VERSION";
const SOURCE_FILES = new Set([VERSION_FILE, "Makefile", "package.json", "bun.lock", "bun.lockb"]);
const GIT_TIMEOUT_MS = 10_000;
const STDIO_PIPE = "pipe";
const GIT_ENV_PREFIX = "GIT_";
const RECORD_SEPARATOR = "\0";
export const HASH_ALGORITHM = "sha256";
export const HASH_ENCODING = "hex";

const SOURCE_MAX_BYTES = 2 * 1024 * 1024;
const TOKENIZER = "tiktoken-rs";
const lockSchema = z.object({ package: z.array(z.object({ name: z.string(), version: z.string() })) });
const POLICY_PATH = "rustd/crates/afd_fleet_runtime/src/definition/mod.rs";
const MIGRATIONS_PATH = "rustd/crates/afd_db/src/migration.rs";
const PROVIDER_PATHS = ["afd_inference/src", "afd_wire/src/inference.rs", "afd_credential/src", "afd_authoring/src/model/provider", "afr_providers/src"];

export const WORKTREE_ROOT = resolve(import.meta.dir, "..");

export function sourceRevision(root = WORKTREE_ROOT): string {
  const result = Bun.spawnSync([GIT, "rev-parse", "HEAD"], {cwd: root, env: gitEnvironment(), timeout: GIT_TIMEOUT_MS});
  if (result.exitCode !== 0) throw new Error("Cannot read build revision");
  return result.stdout.toString().trim();
}

/** Includes untracked source and excludes ignored build outputs. Deleted files vanish from the digest. */
export async function sourceIdentity(root = WORKTREE_ROOT): Promise<string> {
  const listing = Bun.spawnSync([GIT, "ls-files", "--cached", "--others", "--exclude-standard", "-z"], {
    cwd: root, env: gitEnvironment(), stdout: STDIO_PIPE, stderr: STDIO_PIPE, timeout: GIT_TIMEOUT_MS,
  });
  if (listing.exitCode !== 0) throw new Error("Cannot enumerate build sources");
  const paths = [...new Set(listing.stdout.toString().split(RECORD_SEPARATOR))]
    .filter((path) => SOURCE_FILES.has(path) || SOURCE_ROOTS.some((prefix) => path.startsWith(prefix)))
    .sort();
  const digest = new Bun.CryptoHasher(HASH_ALGORITHM);
  for (const path of paths) {
    const file = Bun.file(resolve(root, path));
    if (!await file.exists()) continue;
    digest.update(path).update(RECORD_SEPARATOR).update(await fileHash(file)).update(RECORD_SEPARATOR);
  }
  return digest.digest(HASH_ENCODING);
}

function gitEnvironment(): Record<string, string | undefined> {
  return Object.fromEntries(Object.entries(Bun.env).filter(([name]) => !name.startsWith(GIT_ENV_PREFIX)));
}

export async function fileHash(file: ReturnType<typeof Bun.file>): Promise<string> {
  const digest = new Bun.CryptoHasher(HASH_ALGORITHM);
  for await (const chunk of file.stream()) digest.update(chunk);
  return digest.digest(HASH_ENCODING);
}

export async function outputHash(directory: string): Promise<string> {
  const paths: string[] = [];
  for await (const path of new Bun.Glob("**/*").scan({ cwd: directory, onlyFiles: true, dot: true })) {
    if (!path.startsWith("cache/") && !path.startsWith("dev/")) paths.push(path);
  }
  if (paths.length === 0) throw new Error("Build output is missing");
  const digest = new Bun.CryptoHasher(HASH_ALGORITHM);
  for (const path of paths.sort()) {
    digest.update(path).update(RECORD_SEPARATOR).update(await fileHash(Bun.file(resolve(directory, path)))).update(RECORD_SEPARATOR);
  }
  return digest.digest(HASH_ENCODING);
}


async function sourceText(path: string, root: string): Promise<string> {
  const file = Bun.file(resolve(root, path));
  if (file.size > SOURCE_MAX_BYTES) throw new Error(`Build metadata source exceeds its bound: ${path}`);
  return await file.text();
}

/** Version facts come from the same sources and dependency lock used by this build. */
export async function buildMetadata(root = WORKTREE_ROOT): Promise<z.infer<typeof buildMetadataSchema>> {
  const policy = await sourceText(POLICY_PATH, root);
  const migrations = await sourceText(MIGRATIONS_PATH, root);
  const versions = [...migrations.matchAll(/migration!\("(\d+)_/g)].map((match) => Number(match[1]));
  const lock = lockSchema.parse(Bun.TOML.parse(await sourceText("rustd/Cargo.lock", root)));
  const tokenizer = lock.package.find((entry) => entry.name === TOKENIZER);
  const providerDigest = new Bun.CryptoHasher(HASH_ALGORITHM);
  for (const path of PROVIDER_PATHS) {
    const source = resolve(root, "rustd/crates", path);
    providerDigest.update(path).update(path.endsWith(".rs") ? await fileHash(Bun.file(source)) : await outputHash(source));
  }
  const schemaDigest = new Bun.CryptoHasher(HASH_ALGORITHM)
    .update(await fileHash(Bun.file(resolve(root, MIGRATIONS_PATH))))
    .update(await outputHash(resolve(root, "schema"))).digest(HASH_ENCODING);
  return buildMetadataSchema.parse({
    schema_latest: versions.length ? Math.max(...versions) : undefined, schema_hash: schemaDigest,
    definition_schema_version: Number(policy.match(/pub const DEFINITION_SCHEMA_VERSION: u8 = (\d+);/)?.[1]),
    policy_semantics: policy.match(/pub const POLICY_SEMANTICS: &str = "([^"]+)";/)?.[1],
    tokenizer_version: tokenizer?.version, providers_version: (await sourceText(VERSION_FILE, root)).trim(),
    providers_hash: providerDigest.digest(HASH_ENCODING),
  });
}
