import { z } from "zod";
import type { Development } from "./schema.ts";
import { commit, digest, instant, integer, ref } from "./schema.ts";
import { Records, requireProof } from "./records.ts";

const FIELD = "development.runtime.promotion";
const REPOSITORY = "agentsfleet/agentsfleet";
const API = `https://api.github.com/repos/${REPOSITORY}`;
const RUNNER_FILE = "bin/agentsfleet-runner-linux-amd64";
const fileSchema = z.strictObject({sha256: digest, size: integer.positive()});
export const workflowSchema = z.object({id: integer.positive(), run_attempt: integer.positive(), head_sha: commit,
  event: z.string(), path: z.string(), conclusion: z.string(), repository: z.object({full_name: z.string()}).passthrough(),
  head_repository: z.object({full_name: z.string()}).passthrough(), created_at: instant, run_started_at: instant, updated_at: instant}).passthrough();
export const workflowEvidenceSchema = z.strictObject({read_url: z.url(), response: workflowSchema});
const gitSchema = z.strictObject({read_url: z.url(), response: z.object({sha: commit,
  tree: z.object({sha: commit}).passthrough(), parents: z.array(z.object({sha: commit}).passthrough())}).passthrough()});
const candidateSchema = z.strictObject({schema: z.literal(1), source: z.strictObject({repository: z.string(), version: z.string(),
  source_sha: commit, source_tree: commit, run_head_sha: commit, run_id: integer.positive(), run_attempt: integer.positive(), profile: z.literal("dist")}),
  files: z.record(z.string(), fileSchema)});
const developmentSchema = z.strictObject({schema: z.literal(1), repository: z.string(), source_tree: commit, deployment_sha: commit,
  run_id: integer.positive(), run_attempt: integer.positive(), image: z.string().regex(/^ghcr\.io\/agentsfleet\/agentsfleetd@sha256:[a-f0-9]{64}$/),
  files: z.record(z.string(), fileSchema), gates: z.strictObject({fly: z.literal("success"), metal: z.literal("success"),
    dashboard: z.literal("success"), verify: z.literal("success"), qa: z.literal("success"), acceptance: z.literal("success"), cli: z.literal("success")})});
export const promotionSchema = z.strictObject({
  download: z.strictObject({artifact_id: integer.positive(), archive_sha256: digest,
    entries: z.strictObject({"candidate.json": ref, "development.json": ref})}),
  artifact: z.strictObject({read_url: z.url(), response: z.object({id: integer.positive(), name: z.string(), expired: z.literal(false),
    digest: z.string().regex(/^sha256:[a-f0-9]{64}$/), workflow_run: z.object({id: integer.positive(), head_sha: commit}).passthrough()}).passthrough()}),
  build_run: workflowEvidenceSchema, candidate_commit: gitSchema, deployment_commit: gitSchema,
});
type Promotion = z.infer<typeof promotionSchema>;
type Candidate = z.infer<typeof candidateSchema>;

/** Promotion preserves the tested bytes' build stamp while deploying the equal merge tree. */
export function checkPromotion(runtime: Development["runtime"], records: Records, run: z.infer<typeof workflowEvidenceSchema>) {
  const evidence = records.json(runtime.evidence.promotion, promotionSchema, FIELD);
  const candidateRef = evidence.download.entries["candidate.json"];
  const candidate = records.json(candidateRef, candidateSchema, FIELD);
  const development = records.json(evidence.download.entries["development.json"], developmentSchema, FIELD);
  const retained = development.files["candidate.json"];
  requireProof(retained?.sha256 === candidateRef.sha256 && retained.size === Buffer.byteLength(records.text(candidateRef, FIELD)),
    FIELD, "candidate receipt checksum or byte length differs");
  requireProof(candidate.source.repository === REPOSITORY && development.repository === REPOSITORY
    && development.deployment_sha === runtime.source_revision && development.source_tree === candidate.source.source_tree,
    FIELD, "candidate does not identify the deployed source tree");
  checkWorkflow(run, development.run_id, development.run_attempt, "deploy-dev");
  requireProof(run.response.head_sha === development.deployment_sha && ["push", "workflow_dispatch"].includes(run.response.event),
    FIELD, "deployment run does not own this receipt");
  checkArtifact(evidence, development.run_id, development.deployment_sha, development.source_tree);
  checkCommit(evidence.deployment_commit, development.deployment_sha, development.source_tree);
  checkCommit(evidence.candidate_commit, candidate.source.source_sha, candidate.source.source_tree);
  checkBuild(evidence, candidate, run.response.updated_at);
  const runner = candidate.files[RUNNER_FILE], deployedRunner = development.files[RUNNER_FILE];
  requireProof(runner && deployedRunner && runner.sha256 === deployedRunner.sha256 && runner.size === deployedRunner.size,
    FIELD, "promoted runner differs from the tested candidate");
  return {buildStamp: candidate.source.source_sha.slice(0, 7), version: candidate.source.version, runnerDigest: runner.sha256};
}

function checkWorkflow(run: z.infer<typeof workflowEvidenceSchema>, id: number, attempt: number, name: string): void {
  const data = run.response;
  requireProof(run.read_url === `${API}/actions/runs/${id}` && data.id === id && data.run_attempt === attempt
    && data.repository.full_name === REPOSITORY && data.head_repository.full_name === REPOSITORY
    && data.path === `.github/workflows/${name}.yml` && data.conclusion === "success", FIELD, "workflow identity or attempt differs");
}

function checkArtifact(evidence: Promotion, run: number, revision: string, tree: string): void {
  const artifact = evidence.artifact.response;
  requireProof(evidence.artifact.read_url === `${API}/actions/artifacts/${artifact.id}` && artifact.id === evidence.download.artifact_id
    && artifact.name === `development-candidate-${tree}` && artifact.workflow_run.id === run && artifact.workflow_run.head_sha === revision
    && artifact.digest === `sha256:${evidence.download.archive_sha256}`, FIELD, "download does not identify the deployment's qualified artifact");
}

function checkCommit(record: z.infer<typeof gitSchema>, revision: string, tree: string): void {
  requireProof(record.read_url === `${API}/git/commits/${revision}` && record.response.sha === revision && record.response.tree.sha === tree,
    FIELD, "native Git commit does not identify the retained source tree");
}

function checkBuild(evidence: Promotion, candidate: Candidate, deployedAt: string): void {
  const source = candidate.source, run = evidence.build_run.response;
  checkWorkflow(evidence.build_run, source.run_id, source.run_attempt, "test");
  requireProof(Date.parse(run.updated_at) <= Date.parse(deployedAt), FIELD, "candidate verification postdates deployment");
  if (run.event === "pull_request") {
    const parents = evidence.candidate_commit.response.parents;
    requireProof(parents.length === 2 && parents[1]?.sha === source.run_head_sha && [source.run_head_sha, source.source_sha].includes(run.head_sha),
      FIELD, "candidate is not the tested pull-request merge tree");
  } else {
    requireProof(["merge_group", "workflow_dispatch"].includes(run.event) && run.head_sha === source.source_sha && run.head_sha === source.run_head_sha,
      FIELD, "candidate does not belong to the successful build run");
  }
}
