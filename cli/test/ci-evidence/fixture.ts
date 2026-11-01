import type { Case, Development, Manifest, Ref, Report } from "@ci-evidence/schema";
import { CASE_EXPECTATIONS } from "@ci-evidence/schema";
import { hash, canonical } from "@ci-evidence/records";
import { eventUrl } from "@ci-evidence/native";
import { developmentFixture } from "./development-fixture.ts";

import { AFTER, BEFORE, DRAFT_URL, LOG, LOG_QUOTE, LOG_URL, MODEL, REPOSITORY, RUNNER, SOURCE, TRUNCATED_LOG, WORKSPACE, id } from "./facts.ts";

/** Synthetic unit data: none of these records is a live acceptance witness. */
export class EvidenceFixture {
  report: Report;
  manifest: Manifest = {schema_version: 1, cases: []};
  #next = 0;
  readonly #eventOffset: number;

  constructor(evaluationId = "synthetic-positive", eventOffset = 0) {
    this.#eventOffset = eventOffset;
    this.report = {schema_version: 1, evaluation_id: evaluationId, previous_report_commits: [], source_revision: SOURCE,
      model: structuredClone(MODEL), corpus_sha256: "0".repeat(64), instructions_sha256: {responder: hash("Synthetic responder."), repairer: hash("Synthetic repairer.")},
      records: [], cases: [], development: null};
    for (const [caseId, [role, outcome]] of Object.entries(CASE_EXPECTATIONS)) this.addCase(caseId as Case["case_id"], role, outcome);
    this.report.corpus_sha256 = hash(canonical(this.manifest));
    this.report.development = developmentFixture(this);
  }

  add(value: unknown): Ref {return this.text(canonical(value));}
  eventId(value: number): string {return id(value + this.#eventOffset);}
  text(text: string): Ref {
    const name = `records/synthetic-${this.#next++}.json`;
    this.report.records.push({name, text});
    return {path: name, sha256: hash(text)};
  }
  read(ref: Ref): string {
    const record = this.report.records.find(record => record.name === ref.path);
    if (!record) throw new Error("synthetic record missing");
    return record.text;
  }
  json(ref: Ref): unknown {return JSON.parse(this.read(ref));}
  edit(ref: Ref, path: (string | number)[], value: unknown): void {
    const root = this.json(ref);
    let owner = root;
    for (const key of path.slice(0, -1)) {
      if (typeof owner !== "object" || owner === null) throw new Error("synthetic path missing");
      owner = Reflect.get(owner, key);
    }
    const key = path.at(-1);
    if (typeof owner !== "object" || owner === null || key === undefined) throw new Error("synthetic path missing");
    Reflect.set(owner, key, value);
    this.replace(ref, root);
  }
  replace(ref: Ref, value: unknown): void {this.replaceText(ref, canonical(value));}
  replaceText(ref: Ref, text: string): void {
    const record = this.report.records.find(record => record.name === ref.path);
    if (!record) throw new Error("synthetic record missing");
    record.text = text; ref.sha256 = hash(text);
  }
  subject(index = 0): Case {
    const subject = this.report.cases[index];
    if (!subject) throw new Error("synthetic case missing");
    return subject;
  }
  development(): Development {
    const development = this.report.development;
    if (!development) throw new Error("synthetic development proof missing");
    return development;
  }

  call(subject: Pick<Case, "workspace_id" | "fleet_id" | "event_id">, callId: string, name: string, args: unknown, output: string, status = "succeeded") {
    const read_url = `${eventUrl(subject.workspace_id, subject.fleet_id, subject.event_id)}/tool-calls/${encodeURIComponent(callId)}`;
    return {inventory: {call_id: callId, name, status}, detail: {call_id: callId, read_url,
      record: this.add({read_url, response: {call_id: callId, arguments: args, output, output_line_count: output.split("\n").length, truncated: false, truncated_arguments: false}})}};
  }

  private addCase(caseId: Case["case_id"], role: Case["role"], outcome: Case["outcome"]): void {
    const index = this.report.cases.length;
    const scope = {workspace_id: WORKSPACE, fleet_id: id(role === "responder" ? 20 : 21), event_id: this.eventId(100 + index)};
    const definition = {schema_version: 1, description: "Synthetic setup", instructions_md: `Synthetic ${role}.`, config: {
      name: `ci-${role}`, "x-agentsfleet": {model: "platform-default", tools: ["http_request", "memory_store", "memory_recall"],
        repositories: [REPOSITORY], repository_base: "dev"}}};
    const setup = {id: scope.fleet_id, setup_revision: "1", setup_etag: '"synthetic"', policy_semantics: "fleet-setup-v1", definition};
    const reviewDigest = `authoring:v1:sha256:${role === "responder" ? "a".repeat(64) : "b".repeat(64)}`;
    const input = `Synthetic ${this.report.evaluation_id} input for ${caseId}`;
    const tools = ["diagnosis", "draft", "source_gap"].includes(outcome)
      ? [this.call(scope, "1:8", "http_request", {url: LOG_URL}, caseId === "truncated_required_log" ? TRUNCATED_LOG : outcome === "source_gap" ? "Required source unavailable" : LOG)] : [];
    if (outcome === "draft") tools.push(this.call(scope, "1:9", "http_request", {url: `https://api.github.com/repos/${REPOSITORY}/pulls`, method: "POST"}, DRAFT_URL));
    if (outcome === "diagnosis") tools.push(this.call(scope, "1:9", "memory_store",
      {key: "ci:77:verified", content: "Synthetic persisted finding."}, "remembered ci:77:verified"));
    const answer = outcome === "draft" ? DRAFT_URL : outcome === "diagnosis" ? LOG_QUOTE : outcome === "source_gap" ? "Required source unavailable" : input;
    const witness = tools.length > 0 ? tools[outcome === "draft" ? 1 : 0] : undefined;
    const created_at = `2026-11-01T01:0${index}:00Z`;
    const updated_at = `2026-11-01T01:0${index}:30Z`;
    const event = {...scope, status: "processed", request_json: JSON.stringify({message: input, channel_id: "CFIXTURE", reply_thread_ts: "123.456"}),
      response_text: answer, created_at, updated_at, tool_calls: {calls: tools.map(tool => tool.inventory), omitted_call_count: 0}};
    const policy = hash(canonical({policy_semantics: setup.policy_semantics, config: definition.config}));
    const subject: Case = {...scope, case_id: caseId, variant_id: `synthetic-${role}`, role, outcome, runtime_revision: SOURCE,
      setup_revision: "1", setup_before: this.add(setup), setup_after: this.add(setup), review_digest: reviewDigest, policy_sha256: policy,
      review_snapshot: this.add({review_digest: reviewDigest, policy_semantics: setup.policy_semantics,
        effective: {definition, runtime_model_ref: "platform-default"}}),
      setup_receipt: this.add({read_url: `https://api-dev.agentsfleet.net/v1/workspaces/${WORKSPACE}/fleets`, request: {review_digest: reviewDigest,
        source: {kind: "draft", draft_candidate_id: id(200 + index)}}, response: {id: scope.fleet_id, setup_revision: "1", setup_etag: setup.setup_etag}}),
      model_identity: this.add({captured_at: AFTER, ...scope, leases: [{lease_id: id(300 + index), runner_id: RUNNER, provider: MODEL.provider,
        model: MODEL.id, status: "reported", created_at, updated_at}]}), settings_before: this.settings(BEFORE), settings_after: this.settings(AFTER),
      model_freeze: this.add({operator: "Synthetic operator", agreed_at: BEFORE, at: AFTER, evaluation_id: this.report.evaluation_id,
        model_record_id: id(9), no_changes_during_campaign: true}),
      event_detail: {read_url: eventUrl(scope.workspace_id, scope.fleet_id, scope.event_id), record: this.add(event)},
      prompt: this.text(input), answer: this.text(answer), tool_records: tools.map(tool => tool.detail),
      claims: [{kind: "quote", answer_span: [0, Buffer.byteLength(answer)], witnesses: witness
        ? [{kind: "tool_output", call_id: witness.detail.call_id, span: [witness.detail.call_id === "1:8" && (outcome === "diagnosis" || caseId === "truncated_required_log") ? Buffer.byteLength(caseId === "truncated_required_log" ? TRUNCATED_LOG : LOG) - Buffer.byteLength(answer) : 0,
          witness.detail.call_id === "1:8" && (outcome === "diagnosis" || caseId === "truncated_required_log") ? Buffer.byteLength(caseId === "truncated_required_log" ? TRUNCATED_LOG : LOG) : Buffer.byteLength(answer)]}]
        : [{kind: "case_record", field: "prompt", span: [0, Buffer.byteLength(input)]}]}],
      review: {reviewer: "Synthetic reviewer", at: AFTER, all_claims_reviewed: true, grounding: "pass", stopping_scope: "pass", usefulness: "pass", rationale: "Synthetic known-positive record", findings: []}};
    this.report.cases.push(subject);
    this.manifest.cases.push({case_id: caseId, variant_id: subject.variant_id, role, input, policy_sha256: policy,
      expected_outcome: outcome, required_operations: [], forbidden_effects: [], required_result: "Synthetic expected result"});
  }

  private settings(observed_at: string): Ref {
    return this.add({observed_at, catalogue: {read_url: "https://api-dev.agentsfleet.net/v1/admin/models", response: {models: [{id: id(9),
      provider: MODEL.provider, model_id: MODEL.id, metadata: {context_cap_tokens: MODEL.settings.context_cap_tokens}, inference: MODEL.settings.inference}]}},
      selection: {read_url: `https://api-dev.agentsfleet.net/v1/workspaces/${WORKSPACE}/models`, response: {items: [{ref: "platform-default",
        provider: MODEL.provider, model: MODEL.id, context_window_tokens: MODEL.settings.context_cap_tokens, max_output_tokens: MODEL.settings.inference.max_output_tokens}]}}});
  }
}
