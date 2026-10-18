import {afterEach, beforeEach, expect, test} from "bun:test";
import {mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {resolve} from "node:path";
import {buildMetadata, WORKTREE_ROOT} from "../../scripts/build-source.ts";

const POLICY = "rustd/crates/afd_fleet_runtime/src/definition/mod.rs";
const MIGRATIONS = "rustd/crates/afd_db/src/migration.rs";
const LOCK = "rustd/Cargo.lock";
const PROVIDER = "rustd/crates/afd_inference/src/lib.rs";
const SETTINGS = "rustd/crates/afd_wire/src/inference.rs";
const OVERSIZED_METADATA_BYTES = 2_097_153;
let root: string;

beforeEach(async () => {
  root = await mkdtemp(resolve(tmpdir(), "build-metadata-test-"));
  await Bun.write(resolve(root, "VERSION"), "1.2.3\n");
  await Bun.write(resolve(root, POLICY), 'pub const DEFINITION_SCHEMA_VERSION: u8 = 7;\npub const POLICY_SEMANTICS: &str = "review-v7";');
  await Bun.write(resolve(root, MIGRATIONS), 'migration!("955_before");\nmigration!("956_model_inference");');
  await Bun.write(resolve(root, LOCK), '[[package]]\nname = "tiktoken-rs"\nversion = "0.9.9"\n');
  for (const path of [PROVIDER, SETTINGS, "rustd/crates/afd_credential/src/lib.rs", "rustd/crates/afd_authoring/src/model/provider/mod.rs", "rustd/crates/afr_providers/src/lib.rs", "schema/956_model_inference.sql"]) {
    await Bun.write(resolve(root, path), `source for ${path}`);
  }
});
afterEach(async () => { await rm(root, {recursive: true, force: true}); });

test("metadata reads the actual build's migration, policy, tokenizer and provider sources", async () => {
  const metadata = await buildMetadata(root);
  expect(metadata).toMatchObject({schema_latest:956, definition_schema_version:7,
    policy_semantics:"review-v7", tokenizer_version:"0.9.9", providers_version:"1.2.3"});
  expect(metadata.schema_hash).toMatch(/^[0-9a-f]{64}$/);
  expect(metadata.providers_hash).toMatch(/^[0-9a-f]{64}$/);
  expect(await buildMetadata(root)).toEqual(metadata);
  await Bun.write(resolve(root, SETTINGS), "changed persisted settings shape");
  const changed = await buildMetadata(root);
  expect(changed.providers_hash).not.toBe(metadata.providers_hash);
  expect(changed.schema_hash).toBe(metadata.schema_hash);
  await Bun.write(resolve(root, "schema/956_model_inference.sql"), "changed migration");
  expect((await buildMetadata(root)).schema_hash).not.toBe(metadata.schema_hash);
});

test.each([
  [MIGRATIONS, "no migration declarations"], [POLICY, "no policy declarations"],
  [LOCK, '[[package]]\nname = "another"\nversion = "1"\n'],
  [LOCK, "not valid TOML ["], [POLICY, "x".repeat(OVERSIZED_METADATA_BYTES)],
])("incomplete, malformed or oversized metadata refuses assembly (%s)", async (path, body) => {
  await Bun.write(resolve(root, path), body);
  await expect(buildMetadata(root)).rejects.toThrow();
});

test("the shipping workspace has complete metadata after JSON routing removal", async () => {
  const metadata = await buildMetadata();
  expect(metadata.providers_version).toBe((await Bun.file(resolve(WORKTREE_ROOT, "VERSION")).text()).trim());
  expect(metadata.schema_latest).toBeGreaterThanOrEqual(956);
  expect(metadata.tokenizer_version).toMatch(/^\d+\./);
});
