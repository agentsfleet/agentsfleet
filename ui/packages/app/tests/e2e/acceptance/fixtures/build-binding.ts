import { z } from "zod";
import { inspectBuilds, readAppBuild, readBuildResponse, readProcessRecord, writeBuildEvidence } from "./build-binding-io";
import { BUILD_COMPONENTS, BUILD_HASH_KEYS, buildDigest, buildInspectionSchema, buildManifestSchema, runnerProcessSchema } from "../../../../../../../scripts/build-manifest";

const runnerSchema = z.object({
  id: z.uuid(), status: z.literal("active"), last_seen_at: z.number().int().positive(),
  achievable: z.object({ build_id: buildDigest }),
});
const HEALTH = "/healthz";
const RUNNER = "/v1/runners/me";
const HEARTBEAT_WAIT_MS = 12_000;
const PROCESS_MAX_AGE_MS = 5 * 60_000;

export function assertBuildArtifacts(raw: unknown): z.infer<typeof buildManifestSchema> {
  const inspection = buildInspectionSchema.parse(raw);
  const expected = inspection.manifest;
  if (inspection.source_id !== expected.source_id || inspection.cli.build_id !== expected.source_id) {
    throw new Error("Source or CLI build differs from the acceptance manifest; rebuild the candidate");
  }
  if (inspection.revision !== expected.revision) throw new Error("Acceptance checkout revision differs from its build manifest");
  for (const component of BUILD_COMPONENTS) {
    if (expected.revisions[component] !== expected.revision) throw new Error(`Acceptance ${component} revision differs from the candidate`);
  }
  for (const key of BUILD_HASH_KEYS) {
    if (inspection[key] !== expected[key]) throw new Error(`Acceptance ${key} differs from its build manifest`);
  }
  if (JSON.stringify(inspection.metadata) !== JSON.stringify(expected.metadata)) {
    throw new Error("Acceptance schema, policy, tokenizer or provider metadata differs from its build manifest");
  }
  return expected;
}

export function assertBuildObservation(component: string, observed: unknown, expected: string): void {
  if (!buildDigest.safeParse(observed).success || observed !== expected) {
    throw new Error(`Acceptance ${component} build is unknown or differs from the candidate`);
  }
}

async function processEvidence(expected: z.infer<typeof buildManifestSchema>): Promise<z.infer<typeof runnerProcessSchema>> {
  const evidence = runnerProcessSchema.parse(await readProcessRecord());
  const age = Date.now() - Date.parse(evidence.observed_at);
  if (age < 0 || age > PROCESS_MAX_AGE_MS) throw new Error("Runner process observation is stale");
  assertBuildObservation("runner process", evidence.source_id, expected.source_id);
  if (evidence.binary_hash !== expected.runner_hash) throw new Error("Runner process binary differs from the candidate");
  return evidence;
}

/** Runs before any Clerk setup, bootstrap, cleanup or acceptance fixture mutation. */
export async function requireMatchingBuilds(apiUrl: string, appUrl: string): Promise<void> {
  const expected = assertBuildArtifacts(await inspectBuilds());
  const evidence = await processEvidence(expected);
  const token = process.env.ACCEPTANCE_RUNNER_TOKEN;
  if (!token) throw new Error("ACCEPTANCE_RUNNER_TOKEN is required for read-only heartbeat observation");
  const daemon = z.object({ build_id: buildDigest }).parse((await readBuildResponse(`${apiUrl}${HEALTH}`)).body);
  assertBuildObservation("daemon", daemon.build_id, expected.source_id);
  assertBuildObservation("app", await readAppBuild(appUrl), expected.source_id);
  const first = runnerSchema.parse((await readBuildResponse(`${apiUrl}${RUNNER}`, token)).body);
  assertBuildObservation("runner", first.achievable.build_id, expected.source_id);
  if (first.id !== evidence.runner_id) throw new Error("Observed runner differs from the trusted process record");
  await new Promise((resolveWait) => setTimeout(resolveWait, HEARTBEAT_WAIT_MS));
  const second = runnerSchema.parse((await readBuildResponse(`${apiUrl}${RUNNER}`, token)).body);
  assertBuildObservation("runner", second.achievable.build_id, expected.source_id);
  if (second.id !== first.id || second.last_seen_at <= first.last_seen_at) throw new Error("Candidate runner heartbeat did not advance");
  await writeBuildEvidence(
    `# Acceptance build observations\n\nSource: ${expected.source_id}\nRevision: ${expected.revision}\n` +
    `Environment: ${expected.environment}\nApp: ${new URL(appUrl).origin}\nAPI: ${new URL(apiUrl).origin}\n` +
    `Runner: ${first.id}; process ${evidence.pid} on ${evidence.host}\n` +
    `Runner platform: ${evidence.os}/${evidence.arch}\n` +
    `Heartbeat: ${first.last_seen_at} → ${second.last_seen_at}\n` +
    `Schema: ${expected.metadata.schema_latest}; ${expected.metadata.schema_hash}\n` +
    `Definition schema: ${expected.metadata.definition_schema_version}; policy: ${expected.metadata.policy_semantics}\n` +
    `Tokenizer: ${expected.metadata.tokenizer_version}\n` +
    `Providers: ${expected.metadata.providers_version}; ${expected.metadata.providers_hash}\n` +
    BUILD_COMPONENTS.map((component) => `${component} revision: ${expected.revisions[component]}\n`).join("") +
    BUILD_HASH_KEYS.map((key) => `${key}: ${expected[key]}\n`).join(""));
}

if (import.meta.main) {
  const [apiUrl, appUrl] = process.argv.slice(2);
  if (!apiUrl || !appUrl) throw new Error("Build preflight requires API and app origins");
  await requireMatchingBuilds(apiUrl, appUrl);
}
