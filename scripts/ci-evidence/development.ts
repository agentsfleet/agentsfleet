import type { ZodType } from "zod";
import type { Graded } from "./grade.ts";
import type { Case, Development, Ref } from "./schema.ts";
import { z } from "zod";
import { commit } from "./schema.ts";
import { Records, decode, requireProof } from "./records.ts";
import { eventSchema } from "./native.ts";
import { checkInvestigationReads, jobSchema, runSchema } from "./investigation.ts";
import { checkRegression } from "./regression.ts";
import { checkMemory } from "./memory.ts";
import { checkRuntime } from "./runtime.ts";

const captured = z.strictObject({read_url: z.url(), response: z.json()});
const pullSchema = z.object({html_url: z.url(), draft: z.literal(true), head: z.object({sha: commit, ref: z.string()}).passthrough(),
  base: z.object({sha: commit, ref: z.string()}).passthrough()}).passthrough();
const messageSchema = z.strictObject({permalink: z.url(), channel_id: z.string(), thread_ts: z.string(),
  event_id: z.string(), kind: z.enum(["trigger", "answer", "repair_request", "draft_link"]), text: z.string()});

export function checkDevelopment(graded: Graded): void {
  const {report, records} = graded;
  const development = report.development;
  requireProof(development && graded.grounding === 8 && graded.behavior === 8, "development", "complete passing campaign is required");
  requireProof(development.review.grade === "pass", "development.review", "development review failed or is incomplete");
  const responder = report.cases.find(subject => subject.case_id === development.case_links.responder);
  const repairer = report.cases.find(subject => subject.case_id === development.case_links.repairer);
  requireProof(responder?.case_id === "ci_diagnosis" && repairer?.case_id === "addressed_repair"
    && responder.workspace_id === repairer.workspace_id, "development.case_links", "linked cases do not prove diagnosis and draft in one workspace");
  checkSource(development, records, responder, repairer);
  checkSlack(development, records, responder, repairer);
  requireProof(records.text(development.pull_request.diff, "development.pull_request.diff").trim().length > 0,
    "development.pull_request.diff", "retained draft diff is missing");
  checkRegression(development, records);
  checkRuntime(graded, development);
  checkMemory(graded, development);
  const settled = Math.max(...graded.native.map(native => Date.parse(native.event.updated_at)));
  requireProof(Date.parse(development.review.at) >= settled, "development.review", "review predates campaign settlement");
}

function checkSource(development: Development, records: Records, responder: Case, repairer: Case): void {
  const source = development.source;
  const prefix = `https://api.github.com/repos/${source.repository}`;
  const runUrl = `${prefix}/actions/runs/${source.run_id}`;
  const run = sourceResponse(source.evidence, records, runUrl, runSchema);
  const attempt = sourceResponse(source.evidence, records, `${runUrl}/attempts/${source.attempt}`, runSchema);
  requireProof([run, attempt].every(item => item.id === source.run_id && item.run_attempt === source.attempt
    && item.head_sha === source.head_sha && item.repository.full_name === source.repository), "development.source", "native run or attempt identity differs");
  const job = sourceResponse(source.evidence, records, `${prefix}/actions/jobs/${source.job_id}`, jobSchema);
  requireProof(job.id === source.job_id && job.run_id === source.run_id && job.run_attempt === source.attempt && job.conclusion === "failure"
    && job.steps.some(step => step.name === source.failed_step && step.conclusion === "failure"), "development.source", "native failed job or step differs");
  const pr = development.pull_request;
  const parts = new URL(pr.url).pathname.split("/");
  const number = parts.length === 5 && `${parts[1]}/${parts[2]}` === source.repository && parts[3] === "pull"
    && /^[1-9][0-9]*$/.test(parts[4] ?? "") ? parts[4] : undefined;
  requireProof(number && new URL(pr.url).origin === "https://github.com" && pr.repository === source.repository, "development.pull_request", "draft address or repository differs");
  const native = sourceResponse(source.evidence, records, `${prefix}/pulls/${number}`, pullSchema);
  requireProof(native.html_url === pr.url && native.head.sha === pr.head_sha && native.base.ref === pr.base
    && native.base.sha === development.regression.before.commit
    && native.head.ref === `agentsfleet-repair/${Buffer.from(repairer.event_id).toString("base64url")}`, "development.pull_request", "native draft or tested base differs");
  requireProof(records.text(repairer.answer, "development.pull_request").includes(pr.url), "development.pull_request", "returned draft link differs");
  for (const subject of [responder, repairer]) {
    const fleet = records.json(subject.setup_before, z.object({definition: z.object({config: z.object({"x-agentsfleet": z.object({
      repositories: z.array(z.string()), repository_base: z.string().optional()}).passthrough()}).passthrough()}).passthrough()}).passthrough(), "development.source.setup");
    requireProof(fleet.definition.config["x-agentsfleet"].repositories.includes(source.repository), "development.source", "source is outside accepted repository scope");
    requireProof(subject.role !== "repairer" || fleet.definition.config["x-agentsfleet"].repository_base === pr.base, "development.source", "draft base is outside accepted scope");
    const event = records.json(subject.event_detail.record, eventSchema, "development.source.event");
    checkInvestigationReads(source, {commit: development.regression.before.commit, name: pr.base}, subject.tool_records, event, records, "development.source");
  }
}

function checkSlack(development: Development, records: Records, responder: Case, repairer: Case): void {
  const slack = development.slack;
  const messages = slack.evidence.map(ref => records.json(ref, messageSchema, "development.slack.evidence"));
  for (const [kind, permalink, subject] of [
    ["trigger", slack.trigger_permalink, responder], ["answer", slack.answer_permalink, responder],
    ["repair_request", slack.repair_request_permalink, repairer], ["draft_link", slack.draft_link_permalink, repairer],
  ] as const) {
    const matching = messages.filter(message => message.kind === kind && message.permalink === permalink);
    requireProof(matching.length === 1 && matching[0], "development.slack", "thread result witness is missing or duplicated");
    const message = matching[0];
    requireProof(message.channel_id === slack.channel_id && message.thread_ts === slack.thread_ts
      && message.event_id === subject.event_id, "development.slack", "thread result belongs to another event or address");
    const event = records.json(subject.event_detail.record, z.object({request_json: z.string()}).passthrough(), "development.slack.event");
    const prompt = decode(z.object({channel_id: z.string(), reply_thread_ts: z.string()}).passthrough(), JSON.parse(event.request_json), "development.slack.event");
    requireProof(prompt.channel_id === slack.channel_id && prompt.reply_thread_ts === slack.thread_ts, "development.slack", "native thread address differs");
    const answer = kind === "answer" || kind === "draft_link";
    const expected = records.text(answer ? subject.answer : subject.prompt, "development.slack");
    requireProof(answer ? slackText(message.text) === expected : slackText(message.text).includes(expected.split("\n\nThread (")[0] ?? expected), "development.slack", "visible thread text differs");
  }
}

function sourceResponse<T>(refs: Ref[], records: Records, address: string, schema: ZodType<T>): T {
  const candidates = refs.map(ref => records.json(ref, captured, "development.source.evidence")).filter(record => record.read_url === address);
  requireProof(candidates.length === 1 && candidates[0], "development.source.evidence", "native source address is missing or duplicated");
  return decode(schema, candidates[0].response, "development.source.evidence");
}

function slackText(text: string): string {
  return text.replace(/<(https?:\/\/[^>|]+)(?:\|([^>]*))?>/g, (whole: string, url: string, label: string | undefined) => label === undefined || label === url ? url : whole)
    .replaceAll("&lt;", "<").replaceAll("&gt;", ">").replaceAll("&amp;", "&");
}
