import type { Development } from "@ci-evidence/schema";
import type { EvidenceFixture } from "./fixture.ts";
import { eventUrl } from "@ci-evidence/native";
import { AFTER, BASE, BEFORE, DRAFT_URL, HEAD, LOG, LOG_URL, REPOSITORY, RUNNER, SOURCE, WORKSPACE } from "./facts.ts";
import { addSourceCalls, CODE_BLOB, CODE_PATH, JOBS_URL, regressionFixture, sourceCalls } from "./source-fixture.ts";

export function developmentFixture(fixture: EvidenceFixture): Development {
  const responder = fixture.subject(0);
  const repairer = fixture.subject(6);
  addSourceCalls(fixture, responder);
  addSourceCalls(fixture, repairer);
  const sourcePrefix = `https://api.github.com/repos/${REPOSITORY}`;
  const sourceRun = {id: 77, run_attempt: 1, head_sha: BASE, conclusion: "failure", repository: {full_name: REPOSITORY}};
  const evidence = [
    fixture.add({read_url: `${sourcePrefix}/actions/runs/77`, response: sourceRun}),
    fixture.add({read_url: `${sourcePrefix}/actions/runs/77/attempts/1`, response: sourceRun}),
    fixture.add({read_url: `${sourcePrefix}/actions/jobs/88`, response: {id: 88, run_id: 77, run_attempt: 1,
      conclusion: "failure", steps: [{name: "Check title", conclusion: "failure"}]}}),
    fixture.add({read_url: `${sourcePrefix}/pulls/9`, response: {html_url: DRAFT_URL, draft: true,
      head: {sha: HEAD, ref: `agentsfleet-repair/${Buffer.from(repairer.event_id).toString("base64url")}`}, base: {sha: BASE, ref: "dev"}}}),
  ];
  const channel_id = "CFIXTURE", thread_ts = "123.456";
  const messages = [
    {kind: "trigger", subject: responder}, {kind: "answer", subject: responder},
    {kind: "repair_request", subject: repairer}, {kind: "draft_link", subject: repairer},
  ].map(({kind, subject}, index) => ({kind, event_id: subject.event_id, channel_id, thread_ts,
    permalink: `https://example.slack.com/archives/${channel_id}/p123456${index}`,
    text: fixture.read(kind === "answer" || kind === "draft_link" ? subject.answer : subject.prompt)}));
  const [trigger, answer, request, link] = messages;
  if (!trigger || !answer || !request || !link) throw new Error("synthetic Slack messages missing");
  return {
    case_links: {responder: responder.case_id, repairer: repairer.case_id},
    runtime: runtimeFixture(fixture), source: {repository: REPOSITORY, run_id: 77, attempt: 1, job_id: 88,
      head_sha: BASE, failed_step: "Check title", evidence, jobs_url: JOBS_URL,
      files: [{path: CODE_PATH, source_blob: CODE_BLOB, base_blob: CODE_BLOB}]},
    slack: {channel_id, thread_ts, trigger_permalink: trigger.permalink, answer_permalink: answer.permalink,
      repair_request_permalink: request.permalink, draft_link_permalink: link.permalink, evidence: messages.map(message => fixture.add(message))},
    pull_request: {url: DRAFT_URL, repository: REPOSITORY, base: "dev", draft: true, head_sha: HEAD,
      diff: fixture.text("Synthetic diff: one code fix; unchanged regression assertions.")},
    regression: regressionFixture(fixture),
    review: {reviewer: "Synthetic reviewer", at: AFTER, grade: "pass", rationale: "Synthetic known-positive development joins"},
    memory: memoryFixture(fixture),
  };
}

function runtimeFixture(fixture: EvidenceFixture): Development["runtime"] {
  const workflow_run_url = "https://github.com/agentsfleet/agentsfleet/actions/runs/123";
  const binarySha = "d".repeat(64);
  const process = {pid: 123, start_time: "synthetic-process-start", executable: "/proc/123/exe", sha256: binarySha, build_id: null,
    identity: {read_url: "https://api-dev.agentsfleet.net/v1/runners/me", response: {id: RUNNER}}};
  const probe = fixture.add({runner_id: RUNNER, before: {...process, observed_at: BEFORE}, after: {...process, observed_at: AFTER},
    artifact: {workflow_run_url, artifact_id: 456, archive_name: "synthetic-runner-archive", entry_name: "agentsfleet-runner", sha256: binarySha}});
  const inventory = {read_url: "https://api-dev.agentsfleet.net/v1/fleets/runners?limit=100", response: {
    items: [{id: RUNNER, status: "active", liveness: "online", degraded: false}], next_cursor: null}};
  return {workflow_run_url, source_revision: SOURCE, api_commit_stamp: SOURCE.slice(0, 7),
    runner_observations: [{runner_id: RUNNER, build_id: null, running_sha256: binarySha, artifact_sha256: binarySha, probe}],
    evidence: {
      approval: fixture.add({operator: "Synthetic operator", at: BEFORE, source_revision: SOURCE, workflow_run_url, deploy_runners: true}),
      deployment: fixture.add({workflow_run_url, version: "0.72.0", api_job_ids: [1], runner_job_ids: [2], run: {
        read_url: "https://api.github.com/repos/agentsfleet/agentsfleet/actions/runs/123", response: {head_sha: SOURCE, conclusion: "success", created_at: BEFORE, run_started_at: BEFORE, updated_at: BEFORE}},
        jobs: {read_url: "https://api.github.com/repos/agentsfleet/agentsfleet/actions/runs/123/jobs", response: {jobs: [{id: 1, name: "fly / deploy-fly", conclusion: "success"}, {id: 2, name: "metal / deploy-metal", conclusion: "success"}]}}}),
      health: fixture.add({observed_at: BEFORE, read_url: "https://api-dev.agentsfleet.net/healthz", response: {
        status: "ok", service: "agentsfleetd", version: "0.72.0", commit: SOURCE.slice(0, 7), build_id: null}}),
      readiness: fixture.add({observed_at: BEFORE, read_url: "https://api-dev.agentsfleet.net/readyz", response: {ready: true, database: true, queue: true}}),
      runner: fixture.add({before: {...inventory, observed_at: BEFORE}, after: {...inventory, observed_at: AFTER}, probes: [probe]}),
    }, checks: {approval: "pass", deployment: "pass", health: "pass", readiness: "pass", runner: "pass"}};
}

function memoryFixture(fixture: EvidenceFixture): Development["memory"] {
  const responder = fixture.subject();
  const store = responder.tool_records.find(call => call.call_id === "1:9");
  if (!store) throw new Error("synthetic store call missing");
  const scope = {workspace_id: WORKSPACE, fleet_id: responder.fleet_id, event_id: fixture.eventId(500)};
  const reads = [...sourceCalls(fixture, scope), fixture.call(scope, "1:8", "http_request", {url: LOG_URL}, LOG)];
  const recall = fixture.call(scope, "1:9", "memory_recall", {query: "ci:77"}, "ci:77:verified\nSynthetic persisted finding.");
  const event = fixture.add({...scope, status: "processed", created_at: "2026-11-01T02:00:00Z", updated_at: "2026-11-01T02:00:30Z",
    request_json: JSON.stringify({message: "Synthetic recall request", channel_id: "CFIXTURE", reply_thread_ts: "123.456"}), response_text: "Synthetic recall response",
    tool_calls: {calls: [...reads.map(read => read.inventory), recall.inventory], omitted_call_count: 0}});
  const entryUrl = `https://api-dev.agentsfleet.net/v1/workspaces/${WORKSPACE}/fleets/${responder.fleet_id}/memories`;
  return {store_event: responder.event_detail, recall_event: {read_url: eventUrl(WORKSPACE, responder.fleet_id, scope.event_id), record: event},
    entry: {read_url: entryUrl, record: fixture.add({observed_at: AFTER, read_url: entryUrl, response: {items: [{key: "ci:77:verified", content: "Synthetic persisted finding.",
      writer_fleet_id: responder.fleet_id, visibility: "fleet", category: "core", updated_at: "2026-11-01T01:00:30Z"}]}})},
    store_call: {read_url: store.read_url, record: store.record}, recall_call: {read_url: recall.detail.read_url, record: recall.detail.record},
    recall_tools: [...reads.map(read => read.detail), recall.detail], review: {reviewer: "Synthetic reviewer", at: AFTER, grade: "pass", rationale: "Synthetic persistence and later fresh-read recall"}};
}
