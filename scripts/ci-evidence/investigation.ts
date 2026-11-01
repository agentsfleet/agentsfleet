import type { Case, Development } from "./schema.ts";
import { z } from "zod";
import { callSchema, eventSchema } from "./native.ts";
import { commit, integer } from "./schema.ts";
import { Records, decode, requireProof } from "./records.ts";
import { ownedGet } from "./sources.ts";

export const runSchema = z.object({id: integer, run_attempt: integer, head_sha: commit,
  conclusion: z.literal("failure"), repository: z.object({full_name: z.string()}).passthrough()}).passthrough();
export const jobSchema = z.object({id: integer, run_id: integer, run_attempt: integer, conclusion: z.string().nullable(),
  steps: z.array(z.object({name: z.string(), conclusion: z.string().nullable()}).passthrough())}).passthrough();
const jobsSchema = z.object({total_count: integer, jobs: z.array(jobSchema)}).passthrough();
const contentsSchema = z.object({sha: commit, path: z.string(), content: z.string(), encoding: z.literal("base64")}).passthrough();

/** All required observations must have been read by this event before memory or writes. */
export function checkInvestigationReads(source: Development["source"], base: {commit: string; name: string}, tools: Case["tool_records"],
  event: z.infer<typeof eventSchema>, records: Records, field: string): void {
  const sourceTools = beforeMutations(tools, event, records, field);
  const prefix = `https://api.github.com/repos/${source.repository}`;
  const read = <T>(url: string, schema: z.ZodType<T>): T => {
    let value: unknown;
    try {value = JSON.parse(ownedGet(sourceTools, event, records, url, field));} catch {value = null;}
    return decode(schema, value, field);
  };
  const runUrl = `${prefix}/actions/runs/${source.run_id}`;
  const runs = [read(runUrl, runSchema), read(`${runUrl}/attempts/${source.attempt}`, runSchema)];
  requireProof(runs.every(run => run.id === source.run_id && run.run_attempt === source.attempt
    && run.head_sha === source.head_sha && run.repository.full_name === source.repository), field, "fleet-owned run or attempt differs");
  const job = read(`${prefix}/actions/jobs/${source.job_id}`, jobSchema);
  requireProof(job.id === source.job_id && job.run_id === source.run_id && job.run_attempt === source.attempt && job.conclusion === "failure"
    && job.steps.some(step => step.name === source.failed_step && step.conclusion === "failure"), field, "fleet-owned failed job differs");
  const jobsUrl = new URL(source.jobs_url);
  requireProof(jobsUrl.origin === "https://api.github.com" && jobsUrl.pathname === `/repos/${source.repository}/actions/runs/${source.run_id}/attempts/${source.attempt}/jobs`, field, "selected-attempt job-list address differs");
  const jobs = read(source.jobs_url, jobsSchema);
  requireProof(jobs.total_count === jobs.jobs.length && new Set(jobs.jobs.map(job => job.id)).size === jobs.total_count
    && jobs.jobs.some(item => item.id === source.job_id && item.run_id === job.run_id
      && item.run_attempt === job.run_attempt && item.conclusion === job.conclusion), field, "selected-attempt job list is incomplete or contradictory");
  ownedGet(sourceTools, event, records, `${prefix}/actions/jobs/${source.job_id}/logs`, field, true);
  for (const failed of jobs.jobs.filter(job => job.conclusion === "failure")) {
    const detail = read(`${prefix}/actions/jobs/${failed.id}`, jobSchema);
    requireProof(detail.id === failed.id && detail.run_id === source.run_id && detail.run_attempt === source.attempt
      && detail.conclusion === "failure", field, "failed-job identity differs");
    ownedGet(sourceTools, event, records, `${prefix}/actions/jobs/${failed.id}/logs`, field, true);
  }
  const baseRef = read(`${prefix}/git/ref/heads/${base.name}`, z.object({object: z.object({sha: commit}).passthrough()}).passthrough());
  requireProof(baseRef.object.sha === base.commit, field, "fleet did not verify the current trusted base");
  for (const file of source.files) {
    requireProof(!file.path.startsWith("/") && !file.path.split("/").includes(".."), field, "source path differs");
    for (const [revision, blob] of [[source.head_sha, file.source_blob], [base.commit, file.base_blob]]) {
      const content = read(`${prefix}/contents/${file.path}?ref=${revision}`, contentsSchema);
      requireProof(content.path === file.path && content.sha === blob && content.content.length > 0, field, "fleet-owned tested source or trusted base differs");
    }
  }
}

function beforeMutations(tools: Case["tool_records"], event: z.infer<typeof eventSchema>, records: Records, field: string): Case["tool_records"] {
  const boundary = event.tool_calls?.calls.findIndex(call => {
    if (call.name === "memory_store" || call.name === "memory_recall") return true;
    const detail = tools.find(detail => detail.call_id === call.call_id);
    if (!detail || call.name !== "http_request") return false;
    const args = records.json(detail.record, callSchema, field).response.arguments;
    const request = z.object({method: z.string().optional()}).passthrough().safeParse(args);
    return request.success && !["GET", "HEAD"].includes((request.data.method ?? "GET").toUpperCase());
  }) ?? -1;
  const eligible = new Set((boundary < 0 ? event.tool_calls?.calls : event.tool_calls?.calls.slice(0, boundary))?.map(call => call.call_id));
  return tools.filter(call => eligible.has(call.call_id));
}
