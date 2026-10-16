import {afterEach, beforeEach, expect, test} from "bun:test";
import {resolve} from "node:path";
import {runAcceptanceBuild} from "../../scripts/build-acceptance.ts";
import {ACCEPTANCE_ARTIFACTS, buildAcceptanceComponent} from "../../scripts/acceptance-artifacts.ts";
import {BUILD_COMPONENTS, buildManifestSchema} from "../../scripts/build-manifest.ts";
import {fileHash, outputHash, sourceIdentity, sourceRevision} from "../../scripts/build-source.ts";
import {buildFixture} from "./build-fixture.ts";
import {inspectAcceptanceBuild} from "../../scripts/inspect-acceptance-build.ts";
import {runBuildComponent} from "../../scripts/build-component.ts";

let fixture: Awaited<ReturnType<typeof buildFixture>>;
let previous: Record<string, string | undefined>;
const ENV_KEYS = ["PATH", "ACCEPTANCE_FIXTURE_MODE", "ACCEPTANCE_ENVIRONMENT", "ACCEPTANCE_RUNNER_TARGET", "ACCEPTANCE_BUILD_MANIFEST"];
beforeEach(async () => {
  fixture = await buildFixture();
  previous = Object.fromEntries(ENV_KEYS.map(key => [key, Bun.env[key]]));
  Bun.env.PATH = `${resolve(fixture.root, "bin")}:${Bun.env.PATH}`;
  for (const key of ENV_KEYS.slice(1)) delete Bun.env[key];
});
afterEach(async () => {
  for (const key of ENV_KEYS) { if (previous[key] === undefined) delete Bun.env[key]; else Bun.env[key] = previous[key]; }
  await fixture.cleanup();
});

test.each([undefined, "x86_64-unknown-linux-gnu"])("builds four actual fixture outputs and binds their identities (target %s)", async target => {
  if (target) Bun.env.ACCEPTANCE_RUNNER_TARGET = target;
  await runAcceptanceBuild(undefined, fixture.root);
  const manifest = buildManifestSchema.parse(await Bun.file(resolve(fixture.root, ".tmp/acceptance-build.json")).json());
  expect(manifest.source_id).toBe(await sourceIdentity(fixture.root));
  expect(new Set(Object.values(manifest.revisions))).toEqual(new Set([sourceRevision(fixture.root)]));
  expect(manifest.daemon_hash).toBe(await fileHash(Bun.file(manifest.daemon_path)));
  expect(manifest.runner_hash).toBe(await fileHash(Bun.file(manifest.runner_path)));
  expect(manifest.cli_hash).toBe(await outputHash(resolve(fixture.root, "cli/dist")));
  expect(manifest.app_hash).toBe(await outputHash(resolve(fixture.root, "ui/packages/app/.next")));
  expect(manifest.metadata).toMatchObject({schema_latest:956, providers_version:"1.2.3", tokenizer_version:"0.9.9"});
  expect(manifest.environment).toBe("local");
  const inspected = await inspectAcceptanceBuild(fixture.root);
  expect(inspected.manifest).toEqual(manifest);
  expect(inspected.cli).toEqual({build_id:manifest.source_id});
  expect(inspected.source_id).toBe(manifest.source_id);
  expect(inspected.app_hash).toBe(manifest.app_hash);
});

test.each([
  ["native-failure", "Acceptance native build failed"], ["missing-artifact", "Cargo reported no executable"],
  ["package-failure", "Acceptance build failed"], ["invalid-record", "JSON"],
  ["source-change", "Sources changed during build"], ["revision-change", "Revision changed during build"],
])("a %s cannot produce a manifest", async (mode, message) => {
  Bun.env.ACCEPTANCE_FIXTURE_MODE = mode;
  await expect(runAcceptanceBuild(undefined, fixture.root)).rejects.toThrow(message);
  expect(await Bun.file(resolve(fixture.root, ".tmp/acceptance-build.json")).exists()).toBe(false);
});

test.each([undefined, "review-dev"])("assembles four genuine fixture receipts without rebuilding (environment %s)", async environment => {
  if (environment) Bun.env.ACCEPTANCE_ENVIRONMENT = environment;
  for (const component of BUILD_COMPONENTS) {
    const artifact = ACCEPTANCE_ARTIFACTS[component];
    const output = component === "cli" || component === "app" ? `${artifact}/compiled` : artifact;
    await buildAcceptanceComponent(component, ["bun", "-e", `await Bun.write(${JSON.stringify(output)}, Bun.env.AGENTSFLEET_BUILD_ID)`], {root: fixture.root});
  }
  await runAcceptanceBuild("--assemble", fixture.root);
  const manifest = buildManifestSchema.parse(await Bun.file(resolve(fixture.root, ".tmp/acceptance-build.json")).json());
  expect(manifest.environment).toBe(environment ?? "dev");
  expect(manifest.source_id).toBe(await sourceIdentity(fixture.root));
  expect(manifest.runner_hash).toBe(await fileHash(Bun.file(resolve(fixture.root, ACCEPTANCE_ARTIFACTS.runner))));
});

test("the component command validates its component and forwards the physical build arguments", async () => {
  const path = ACCEPTANCE_ARTIFACTS.runner;
  await runBuildComponent(["runner", "bun", "-e", `await Bun.write(${JSON.stringify(path)}, Bun.env.AGENTSFLEET_BUILD_ID)`], fixture.root);
  expect(await Bun.file(resolve(fixture.root, "dist/acceptance-components/runner.json")).json()).toMatchObject({component:"runner"});
  await expect(runBuildComponent(["missing"], fixture.root)).rejects.toThrow();
});

test.each(["missing", "oversized", "failed-cli", "oversized-cli", "invalid-cli"])("inspection rejects %s evidence", async kind => {
  const manifest = resolve(fixture.root, ".tmp/acceptance-build.json");
  if (kind !== "missing" && kind !== "oversized") await runAcceptanceBuild(undefined, fixture.root);
  if (kind === "oversized") await Bun.write(manifest, "x".repeat(65_537));
  if (kind.endsWith("cli")) {
    const command = kind === "failed-cli" ? "process.exit(3)" : kind === "oversized-cli" ? 'console.log("x".repeat(1025))' : 'console.log("not JSON")';
    await Bun.write(resolve(fixture.root, "cli/dist/bin/agentsfleet.js"), command);
  }
  Bun.env.ACCEPTANCE_BUILD_MANIFEST = manifest;
  await expect(inspectAcceptanceBuild(fixture.root)).rejects.toThrow();
});
