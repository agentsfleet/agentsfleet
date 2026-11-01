import type { Development } from "./schema.ts";
import { z } from "zod";
import { commit, integer } from "./schema.ts";
import { Records, canonical, requireProof } from "./records.ts";

const capturedRun = z.object({read_url: z.url(), response: z.object({id: integer.positive(), head_sha: commit,
  conclusion: z.enum(["success", "failure"]), repository: z.object({full_name: z.string()}).passthrough()}).passthrough()});
const capturedJob = z.object({read_url: z.url(), response: z.object({id: integer.positive(), run_id: integer.positive(),
  name: z.string(), conclusion: z.enum(["success", "failure"])}).passthrough()});

/** The same hashed test must execute at both pinned revisions, with native results. */
export function checkRegression(development: Development, records: Records): void {
  const {fixture, before, after} = development.regression;
  const field = "development.regression";
  const contents = records.text(fixture.content, field);
  requireProof(contents.trim().length > 0 && fixture.repository === development.source.repository, field, "unchanged fixture is missing or belongs to another repository");
  requireProof(before.command === after.command && before.exit_status !== 0 && before.counts.failed > 0 && before.counts.skipped === 0
    && after.exit_status === 0 && after.counts.failed === 0 && after.counts.skipped === 0 && after.counts.passed > 0
    && before.counts.passed + before.counts.failed === after.counts.passed, field, "unchanged regression was not red before and green after");
  requireProof(after.commit === development.pull_request.head_sha, field, "tested revision differs from the draft head");
  for (const result of [before, after]) checkExecution(result, fixture, records);
}

function checkExecution(result: Development["regression"]["before"], fixture: Development["regression"]["fixture"], records: Records): void {
  const field = "development.regression.execution";
  const run = records.json(result.execution.run.record, capturedRun, field);
  const job = records.json(result.execution.job.record, capturedJob, field);
  const prefix = `https://api.github.com/repos/${fixture.repository}`;
  const conclusion = result.exit_status === 0 ? "success" : "failure";
  requireProof(result.execution.run.read_url === run.read_url && run.read_url === `${prefix}/actions/runs/${run.response.id}`
    && run.response.head_sha === result.commit && run.response.repository.full_name === fixture.repository
    && run.response.conclusion === conclusion, field, "native regression run or tested revision differs");
  requireProof(result.execution.job.read_url === job.read_url && job.read_url === `${prefix}/actions/jobs/${job.response.id}`
    && job.response.run_id === run.response.id && job.response.name === result.executor && job.response.conclusion === conclusion
    && result.execution.log_url === `${job.read_url}/logs`, field, "native regression job or log address differs");
  const log = records.text(result.output, field).replace(/\u001b\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/^\d{4}-\d\d-\d\dT[^ ]+ /gm, "");
  requireProof(log.split("\n").some(line => line.trim() === result.commit)
    && log.includes(`##[group]Run ${result.command}\n`) && log.includes(`${fixture.content.sha256}  ${fixture.path}`)
    && result.command.endsWith(` ${fixture.path}`), field, "execution did not record the command, tested commit and unchanged fixture hash");
  const summaries = [...log.matchAll(/^\s*Tests\s+(.+?)\s*\(([0-9]+)\)\s*$/gm)];
  const summary = summaries[0];
  requireProof(summaries.length === 1 && summary, field, "regression result summary is missing or ambiguous");
  const counts = {passed: 0, failed: 0, skipped: 0};
  for (const match of (summary[1] ?? "").matchAll(/([0-9]+) (passed|failed|skipped)/g)) {
    const key = match[2]; if (key === "passed" || key === "failed" || key === "skipped") counts[key] = Number(match[1]);
  }
  requireProof(canonical(counts) === canonical(result.counts) && Object.values(counts).reduce((sum, value) => sum + value, 0) === Number(summary[2]), field, "retained test output contradicts declared results");
}
