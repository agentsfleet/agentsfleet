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
  const jobs = readJobPages(source, sourceTools, event, records, field);
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

/** A complete listing can span pages; every retained page belongs to this exact attempt. */
function readJobPages(source: Development["source"], tools: Case["tool_records"], event: z.infer<typeof eventSchema>, records: Records, field: string) {
  const initial = new URL(source.jobs_url);
  const pageSize = Number(initial.searchParams.get("per_page") ?? "30");
  requireProof(Number.isInteger(pageSize) && pageSize >= 1 && pageSize <= 100
    && (initial.searchParams.get("page") ?? "1") === "1"
    && [...initial.searchParams.keys()].every(key => ["page", "per_page"].includes(key))
    && [...new Set(initial.searchParams.keys())].every(key => initial.searchParams.getAll(key).length === 1), field, "job-list paging differs");
  const read = (address: string) => {
    let value: unknown;
    try {value = JSON.parse(ownedGet(tools, event, records, address, field));} catch {value = null;}
    return decode(jobsSchema, value, field);
  };
  const first = read(source.jobs_url);
  const pages = new Map<number, string>([[1, source.jobs_url]]);
  for (const call of tools) {
    const detail = records.json(call.record, callSchema, field);
    const args = z.object({url: z.url()}).passthrough().safeParse(detail.response.arguments);
    if (!args.success) continue;
    const url = new URL(args.data.url);
    if (url.origin !== initial.origin || url.pathname !== initial.pathname) continue;
    const page = Number(url.searchParams.get("page") ?? "1");
    requireProof(Number.isInteger(page) && page >= 1 && Number(url.searchParams.get("per_page") ?? "30") === pageSize
      && [...url.searchParams.keys()].every(key => ["page", "per_page"].includes(key))
      && [...new Set(url.searchParams.keys())].every(key => url.searchParams.getAll(key).length === 1), field, "job-list paging differs");
    requireProof(!pages.has(page) || pages.get(page) === args.data.url, field, "job-list page identity differs");
    pages.set(page, args.data.url);
  }
  const pageCount = Math.max(1, Math.ceil(first.total_count / pageSize));
  requireProof(pages.size >= pageCount && pages.size <= pageCount + 1, field, "job-list page inventory is incomplete or contradictory");
  if (pages.size > pageCount) {
    const address = pages.get(pageCount + 1);
    requireProof(address !== undefined, field, "terminal job-list page differs");
    const terminal = read(address);
    requireProof(terminal.total_count === first.total_count && terminal.jobs.length === 0, field, "terminal job-list page is not empty or its total differs");
  }
  const jobs: z.infer<typeof jobSchema>[] = [];
  for (let page = 1; page <= pageCount; page++) {
    const address = pages.get(page);
    requireProof(address !== undefined, field, "job-list page is missing");
    const result = page === 1 ? first : read(address);
    requireProof(result.total_count === first.total_count && result.jobs.length === Math.min(pageSize, first.total_count - jobs.length)
      && result.jobs.every(job => job.run_id === source.run_id && job.run_attempt === source.attempt), field, "job-list page is incomplete or belongs to another attempt");
    jobs.push(...result.jobs);
  }
  return {total_count: first.total_count, jobs};
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
