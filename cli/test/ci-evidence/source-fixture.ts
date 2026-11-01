import type { Case, Development } from "@ci-evidence/schema";
import type { EvidenceFixture } from "./fixture.ts";
import { eventSchema } from "@ci-evidence/native";
import { BASE, REPOSITORY } from "./facts.ts";

export const CODE_PATH = "apps/web/lib/client/unescapeString.ts";
export const CODE_BLOB = "e".repeat(40);
export const PREFIX = `https://api.github.com/repos/${REPOSITORY}`;
export const SOURCE_RUN = {id: 77, run_attempt: 1, head_sha: BASE, conclusion: "failure", repository: {full_name: REPOSITORY}};
export const SOURCE_JOB = {id: 88, run_id: 77, run_attempt: 1, conclusion: "failure", steps: [{name: "Check title", conclusion: "failure"}]};
export const JOBS_URL = `${PREFIX}/actions/runs/77/attempts/1/jobs?per_page=100&page=1`;

export function sourceCalls(fixture: EvidenceFixture, scope: Pick<Case, "workspace_id" | "fleet_id" | "event_id">) {
  const content = {path: CODE_PATH, sha: CODE_BLOB, content: Buffer.from("Synthetic tested source.").toString("base64"), encoding: "base64"};
  // Source and base use the same revision in this synthetic failing fixture.
  return [
    [`${PREFIX}/actions/runs/77`, SOURCE_RUN], [`${PREFIX}/actions/runs/77/attempts/1`, SOURCE_RUN],
    [JOBS_URL, {total_count: 1, jobs: [SOURCE_JOB]}], [`${PREFIX}/actions/jobs/88`, SOURCE_JOB],
    [`${PREFIX}/git/ref/heads/dev`, {object: {sha: BASE}}],
    [`${PREFIX}/contents/${CODE_PATH}?ref=${BASE}`, content],
  ].map(([url, body], index) => fixture.call(scope, `1:${index + 1}`, "http_request", {url}, `Status: 200\nContent-Type: application/json\n\nResponse Body:\n${JSON.stringify(body)}`));
}

export function addSourceCalls(fixture: EvidenceFixture, subject: Case): void {
  const calls = sourceCalls(fixture, subject);
  const event = eventSchema.parse(fixture.json(subject.event_detail.record));
  subject.tool_records.unshift(...calls.map(call => call.detail));
  fixture.edit(subject.event_detail.record, ["tool_calls", "calls"], [...calls.map(call => call.inventory), ...(event.tool_calls?.calls ?? [])]);
}

export function regressionFixture(fixture: EvidenceFixture): Development["regression"] {
  const path = "apps/web/lib/client/unescapeString.test.ts";
  const content = fixture.text("Synthetic unchanged regression: three assertions.");
  const command = `yarn test run ${path}`;
  const executor = "title-regression";
  const execution = (commit: string, passed: number, failed: number, runId: number, jobId: number) => {
    const conclusion = failed > 0 ? "failure" : "success";
    const runUrl = `${PREFIX}/actions/runs/${runId}`, jobUrl = `${PREFIX}/actions/jobs/${jobId}`;
    return {commit, command, executor, exit_status: failed > 0 ? 1 : 0, counts: {passed, failed, skipped: 0},
      output: fixture.text(`${commit}\n${content.sha256}  ${path}\n##[group]Run ${command}\n Tests ${failed ? `${failed} failed | ` : ""}${passed} passed (${passed + failed})\n`),
      execution: {run: {read_url: runUrl, record: fixture.add({read_url: runUrl, response: {id: runId, head_sha: commit, conclusion, repository: {full_name: REPOSITORY}}})},
        job: {read_url: jobUrl, record: fixture.add({read_url: jobUrl, response: {id: jobId, run_id: runId, name: executor, conclusion}})}, log_url: `${jobUrl}/logs`}};
  };
  return {fixture: {repository: REPOSITORY, path, source_revision: BASE, content},
    before: execution(BASE, 2, 1, 100, 1000), after: execution("c".repeat(40), 3, 0, 101, 1001)};
}
