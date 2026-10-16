import { z } from "zod";

export const BUILD_RECORD_MAX_BYTES = 64 * 1024;
export const BUILD_HASH_KEYS = ["daemon_hash", "runner_hash", "cli_hash", "app_hash"] as const;
export const BUILD_COMPONENTS = ["daemon", "runner", "cli", "app"] as const;
export const buildDigest = z.string().regex(/^[0-9a-f]{64}$/);
const revision = z.string().regex(/^[0-9a-f]{40}$/);
const path = z.string().min(1).max(4096);
const version = z.string().min(1).max(128);

export const buildMetadataSchema = z.object({
  schema_latest: z.number().int().positive(), schema_hash: buildDigest,
  definition_schema_version: z.number().int().positive(), policy_semantics: version,
  tokenizer_version: version, providers_version: version, providers_hash: buildDigest,
});
export const buildHashesSchema = z.object({
  daemon_hash: buildDigest, runner_hash: buildDigest, cli_hash: buildDigest, app_hash: buildDigest,
});
export const buildManifestSchema = buildHashesSchema.extend({
  source_id: buildDigest, revision,
  revisions: z.object({daemon: revision, runner: revision, cli: revision, app: revision}),
  built_at: z.iso.datetime(), environment: z.string().min(1).max(128),
  daemon_path: path, runner_path: path, metadata: buildMetadataSchema,
});
export const buildInspectionSchema = buildHashesSchema.extend({
  source_id: buildDigest, revision, manifest: buildManifestSchema, metadata: buildMetadataSchema,
  cli: z.object({ build_id: buildDigest }),
});
export const runnerProcessSchema = z.object({
  runner_id: z.uuid(), source_id: buildDigest, binary_hash: buildDigest,
  pid: z.number().int().positive(), host: z.string().min(1).max(256), observed_at: z.iso.datetime(),
  os: z.literal("linux"), arch: z.enum(["arm64", "x64"]),
});

export type BuildManifest = z.infer<typeof buildManifestSchema>;
