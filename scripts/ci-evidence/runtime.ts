import type { Development } from "./schema.ts";
import type { Graded } from "./grade.ts";
import { z } from "zod";
import { commit, digest, identifier, instant, integer, ref } from "./schema.ts";
import { Records, canonical, requireProof } from "./records.ts";
import { identitySchema } from "./native.ts";
import { checkPromotion, workflowEvidenceSchema } from "./promotion.ts";

const approvalSchema = z.strictObject({operator: z.string().min(1), at: instant, source_revision: commit,
  workflow_run_url: z.url(), deploy_runners: z.literal(true)});
const runSchema = workflowEvidenceSchema;
const deploymentSchema = z.strictObject({workflow_run_url: z.url(), version: z.string().min(1), run: runSchema,
  api_job_ids: z.array(integer.positive()).min(1), runner_job_ids: z.array(integer.positive()).min(1),
  jobs: z.strictObject({read_url: z.url(), response: z.object({jobs: z.array(z.object({id: integer.positive(),
    name: z.string(), conclusion: z.string().nullable()}).passthrough())}).passthrough()})});
const healthSchema = z.strictObject({observed_at: instant, read_url: z.url(), response: z.object({status: z.literal("ok"),
  service: z.literal("agentsfleetd"), version: z.string(), commit: z.string(), build_id: z.string().nullable()}).passthrough()});
const readySchema = z.strictObject({observed_at: instant, read_url: z.url(), response: z.strictObject({
  ready: z.literal(true), database: z.literal(true), queue: z.literal(true)})});
const inventorySchema = z.strictObject({observed_at: instant, read_url: z.url(), response: z.object({
  items: z.array(z.object({id: identifier, status: z.string(), liveness: z.string(), degraded: z.boolean()}).passthrough()),
  next_cursor: z.null()}).passthrough()});
const runnersSchema = z.strictObject({before: inventorySchema, after: inventorySchema, probes: z.array(ref).min(1)});
const processSchema = z.strictObject({observed_at: instant, pid: integer.positive(), start_time: z.string().min(1),
  executable: z.string(), sha256: digest, build_id: z.string().nullable(),
  identity: z.strictObject({read_url: z.url(), response: z.object({id: identifier}).passthrough()})});
const probeSchema = z.strictObject({runner_id: identifier, before: processSchema, after: processSchema,
  artifact: z.strictObject({workflow_run_url: z.url(), artifact_id: integer.positive(), archive_name: z.string().min(1),
    entry_name: z.literal("agentsfleet-runner"), sha256: digest})});

export function checkRuntime(graded: Graded, development: Development): void {
  const {report, records} = graded;
  const runtime = development.runtime;
  requireProof(runtime.source_revision === report.source_revision, "development.runtime", "source revision differs");
  requireProof(Object.values(runtime.checks).every(grade => grade === "pass"), "development.runtime.checks", "runtime check failed or is incomplete");
  const approval = records.json(runtime.evidence.approval, approvalSchema, "development.runtime.approval");
  requireProof(approval.source_revision === report.source_revision && approval.workflow_run_url === runtime.workflow_run_url, "development.runtime.approval", "approved deployment differs");
  const deployment = records.json(runtime.evidence.deployment, deploymentSchema, "development.runtime.deployment");
  requireProof(deployment.workflow_run_url === runtime.workflow_run_url && deployment.run.response.head_sha === report.source_revision
    && deployment.run.response.conclusion === "success", "development.runtime.deployment", "enabled deployment did not succeed for the approved revision");
  const run = new URL(runtime.workflow_run_url).pathname.match(/^\/agentsfleet\/agentsfleet\/actions\/runs\/(\d+)$/)?.[1];
  requireProof(run && new URL(runtime.workflow_run_url).origin === "https://github.com", "development.runtime", "deployment run address differs");
  const address = `https://api.github.com/repos/agentsfleet/agentsfleet/actions/runs/${run}`;
  requireProof(deployment.run.read_url === address && deployment.jobs.read_url === `${address}/jobs`, "development.runtime.deployment", "native deployment address differs");
  const jobs = [...deployment.api_job_ids, ...deployment.runner_job_ids];
  requireProof(new Set(jobs).size === jobs.length && jobs.every(id => deployment.jobs.response.jobs.some(job => job.id === id && job.conclusion === "success")), "development.runtime.deployment", "API or runner deployment job was skipped or failed");
  requireProof(deployment.api_job_ids.every(id => deployment.jobs.response.jobs.some(job => job.id === id && job.name === "fly / deploy-fly"))
    && deployment.runner_job_ids.every(id => deployment.jobs.response.jobs.some(job => job.id === id && job.name === "metal / deploy-metal")), "development.runtime.deployment", "deployment job IDs do not name the API and runner jobs");
  const health = records.json(runtime.evidence.health, healthSchema, "development.runtime.health");
  const build = checkPromotion(runtime, records, deployment.run);
  requireProof(health.read_url === "https://api-dev.agentsfleet.net/healthz" && health.response.commit === runtime.api_commit_stamp
    && runtime.api_commit_stamp === build.buildStamp && health.response.version === deployment.version && build.version === deployment.version,
    "development.runtime.health", "observed API build differs");
  const ready = records.json(runtime.evidence.readiness, readySchema, "development.runtime.readiness");
  requireProof(ready.read_url === "https://api-dev.agentsfleet.net/readyz", "development.runtime.readiness", "readiness address differs");
  const first = Math.min(...graded.native.map(native => Date.parse(native.event.created_at)));
  const last = Math.max(...graded.native.map(native => Date.parse(native.event.updated_at)));
  requireProof(Date.parse(approval.at) <= first && Date.parse(deployment.run.response.updated_at) <= first
    && Date.parse(health.observed_at) <= first && Date.parse(ready.observed_at) <= first, "development.runtime", "runtime prerequisites postdate the campaign");
  const deployed = Date.parse(deployment.run.response.updated_at);
  requireProof(Date.parse(approval.at) <= Date.parse(deployment.run.response.created_at)
    && Date.parse(deployment.run.response.created_at) <= Date.parse(deployment.run.response.run_started_at)
    && Date.parse(deployment.run.response.run_started_at) <= deployed
    && Date.parse(health.observed_at) >= deployed && Date.parse(ready.observed_at) >= deployed,
    "development.runtime", "approval or health observations do not bracket the deployment");
  checkRunners(runtime, records, first, last, build.runnerDigest);
  const observed = new Set(runtime.runner_observations.map(row => row.runner_id));
  for (const subject of report.cases) {
    const identity = records.json(subject.model_identity, identitySchema, "development.runtime.leases");
    requireProof(identity.leases.every(lease => observed.has(lease.runner_id)), "development.runtime.leases", "event used an unattested runner");
  }
}

function checkRunners(runtime: Development["runtime"], records: Records, first: number, last: number, runnerDigest: string): void {
  const retained = records.json(runtime.evidence.runner, runnersSchema, "development.runtime.runner");
  const active = (inventory: z.infer<typeof inventorySchema>) => {
    requireProof(new URL(inventory.read_url).origin === "https://api-dev.agentsfleet.net"
      && new URL(inventory.read_url).pathname === "/v1/fleets/runners" && !new URL(inventory.read_url).searchParams.has("starting_after"), "development.runtime.runner", "runner inventory address differs");
    const rows = inventory.response.items.filter(row => row.status === "active" && ["online", "busy"].includes(row.liveness));
    requireProof(rows.every(row => !row.degraded), "development.runtime.runner", "active runner is degraded");
    return rows.map(row => row.id).sort();
  };
  const before = active(retained.before);
  const after = active(retained.after);
  const observed = runtime.runner_observations.map(row => row.runner_id).sort();
  requireProof(before.length > 0 && canonical(before) === canonical(after) && canonical(before) === canonical(observed)
    && new Set(observed).size === observed.length && runtime.runner_observations.every(row => row.artifact_sha256 === runnerDigest),
    "development.runtime.runner", "active inventory changed or is not fully attested");
  requireProof(Date.parse(retained.before.observed_at) <= first && Date.parse(retained.after.observed_at) >= last, "development.runtime.runner", "inventory does not bracket the campaign");
  requireProof(retained.probes.length === observed.length && new Set(retained.probes.map(probe => canonical(probe))).size === observed.length, "development.runtime.runner", "host probe inventory is incomplete");
  for (const row of runtime.runner_observations) {
    requireProof(retained.probes.some(probe => canonical(probe) === canonical(row.probe)), "development.runtime.runner", "host probe belongs to another inventory");
    const probe = records.json(row.probe, probeSchema, "development.runtime.runner.probe");
    requireProof(probe.runner_id === row.runner_id && probe.artifact.workflow_run_url === runtime.workflow_run_url
      && probe.artifact.sha256 === row.artifact_sha256 && row.running_sha256 === row.artifact_sha256, "development.runtime.runner", "running and downloaded executables differ");
    requireProof(probe.before.pid === probe.after.pid && probe.before.start_time === probe.after.start_time, "development.runtime.runner", "runner process changed during the campaign");
    for (const process of [probe.before, probe.after]) {
      requireProof(process.executable === `/proc/${process.pid}/exe` && process.sha256 === row.running_sha256
        && process.build_id === row.build_id && process.identity.response.id === row.runner_id
        && process.identity.read_url === "https://api-dev.agentsfleet.net/v1/runners/me", "development.runtime.runner", "host identity or executable differs");
    }
    requireProof(Date.parse(probe.before.observed_at) <= first && Date.parse(probe.after.observed_at) >= last, "development.runtime.runner", "host probes do not bracket the campaign");
  }
}
