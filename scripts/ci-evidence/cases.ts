import type { Case, Report } from "./schema.ts";
import { z } from "zod";
import { Records, bytesAt, canonical, decode, hash, requireProof } from "./records.ts";
import { callSchema, eventSchema, eventUrl, fleetSchema, receiptSchema, requestSchema, reviewSchema } from "./native.ts";
import { expectedRefAbsence, failedSource, intentionalLogTruncation, terminalRefusal } from "./sources.ts";
import { checkProjection } from "./projection.ts";

export function checkCase(subject: Case, report: Report, records: Records, field: string) {
  const before = records.json(subject.setup_before, fleetSchema, `${field}.setup_before`);
  const after = records.json(subject.setup_after, fleetSchema, `${field}.setup_after`);
  const selected = (fleet: typeof before) => ({id: fleet.id, revision: fleet.setup_revision,
    tag: fleet.setup_etag, semantics: fleet.policy_semantics, definition: fleet.definition});
  requireProof(canonical(selected(before)) === canonical(selected(after)), field, "accepted setup changed during the event");
  requireProof(before.id === subject.fleet_id && before.setup_revision === subject.setup_revision, field, "accepted setup identity differs");
  const policy = hash(canonical({policy_semantics: before.policy_semantics, config: before.definition.config}));
  requireProof(policy === subject.policy_sha256, `${field}.policy_sha256`, "accepted policy differs");
  requireProof(hash(before.definition.instructions_md) === report.instructions_sha256[subject.role], field, "accepted instructions differ");
  checkReceipt(subject, before, records, field);
  const event = records.json(subject.event_detail.record, eventSchema, `${field}.event_detail`);
  const url = eventUrl(subject.workspace_id, subject.fleet_id, subject.event_id);
  requireProof(subject.event_detail.read_url === url, field, "event address differs");
  requireProof(event.workspace_id === subject.workspace_id && event.fleet_id === subject.fleet_id && event.event_id === subject.event_id, field, "native event identity differs");
  requireProof(event.status === "processed" && event.response_text === records.text(subject.answer, field), field, "native answer is unavailable or differs");
  let request: unknown;
  try {request = JSON.parse(event.request_json);} catch {request = null;}
  const prompt = decode(requestSchema, request, `${field}.event_detail.request_json`);
  requireProof(prompt.message === records.text(subject.prompt, field), field, "composed native prompt differs");
  requireProof(subject.runtime_revision === report.source_revision, field, "runtime revision differs");
  const outputs = checkCalls(subject, event, records, url, field);
  if (["diagnosis", "draft", "source_gap"].includes(subject.outcome)) {
    requireProof(outputs.size > 0 && subject.claims.some(claim => claim.witnesses.some(witness => witness.kind === "tool_output")), field, "external-source outcome has no tool witness");
  }
  checkClaims(subject, outputs, records, field);
  requireProof(Date.parse(subject.review.at) >= Date.parse(event.updated_at), field, "review predates settlement");
  return {event, fleet: before, prompt, outputs};
}

function checkReceipt(subject: Case, before: z.infer<typeof fleetSchema>, records: Records, field: string): void {
  const review = records.json(subject.review_snapshot, reviewSchema, `${field}.review_snapshot`);
  const receipt = records.json(subject.setup_receipt, receiptSchema, `${field}.setup_receipt`);
  requireProof(review.review_digest === subject.review_digest && receipt.request.review_digest === subject.review_digest, field, "review digest differs");
  requireProof(canonical(review.effective.definition) === canonical(before.definition)
    && review.policy_semantics === before.policy_semantics, field, "reviewed setup differs");
  requireProof(review.effective.runtime_model_ref === before.definition.config["x-agentsfleet"].model, field, "reviewed model differs");
  requireProof(receipt.response.id === subject.fleet_id && receipt.response.setup_revision === subject.setup_revision
    && receipt.response.setup_etag === before.setup_etag, field, "native receipt identity differs");
  requireProof(Boolean(receipt.request.source?.draft_candidate_id || receipt.request.draft_candidate_id), field, "receipt has no reviewed candidate");
  const prefix = `https://api-dev.agentsfleet.net/v1/workspaces/${subject.workspace_id}/fleets`;
  requireProof(receipt.read_url === prefix || receipt.read_url === `${prefix}/${subject.fleet_id}/revisions`, field, "receipt address differs");
}

export function checkCalls(subject: Pick<Case, "tool_records" | "outcome"> & Partial<Pick<Case, "case_id">>, event: z.infer<typeof eventSchema>, records: Records, url: string, field: string): Map<string, string> {
  requireProof(event.tool_calls !== null && event.tool_calls.omitted_call_count === 0, field, "native tool trace is unavailable or incomplete");
  const inventory = new Map(event.tool_calls.calls.map(call => [call.call_id, call]));
  requireProof(new Set(event.tool_calls.calls.map(call => call.call_id.slice(0, call.call_id.indexOf(":")))).size <= 1,
    field, "native tool trace mixes lease fences");
  if ("case_id" in subject && (subject.case_id === "missing_link" || subject.case_id === "foreign_repository")) {
    requireProof(inventory.size === 0, field, "prompt-only clarification or refusal made a tool call");
  }
  requireProof(inventory.size === event.tool_calls.calls.length && inventory.size === subject.tool_records.length, field, "tool inventory is duplicated or incomplete");
  const outputs = new Map<string, string>();
  const details = new Map<string, z.infer<typeof callSchema>>();
  for (const call of subject.tool_records) {
    requireProof(inventory.has(call.call_id) && !outputs.has(call.call_id), field, "tool identity is duplicated or belongs to another case");
    const detail = records.json(call.record, callSchema, `${field}.tool_records`);
    requireProof(detail.response.call_id === call.call_id && detail.read_url === call.read_url
      && call.read_url === `${url}/tool-calls/${encodeURIComponent(call.call_id)}`, field, "tool identity or address differs");
    requireProof(!detail.response.truncated_arguments, field, "tool arguments are incomplete");
    const saved = inventory.get(call.call_id);
    requireProof(saved, field, "native inventory entry is missing");
    checkProjection(saved, detail.response, field);
    requireProof(!detail.response.truncated || (subject.case_id === "truncated_required_log" && intentionalLogTruncation(detail)), field, "required tool output is incomplete");
    outputs.set(call.call_id, detail.response.output);
    details.set(call.call_id, detail);
  }
  const calls = event.tool_calls.calls;
  for (let index = 0; index < calls.length - 1; index++) {
    const call = calls[index], next = calls[index + 1];
    const detail = details.get(call?.call_id ?? "");
    requireProof(call && detail, field, "native tool detail is missing");
    const stop = terminalRefusal(detail.response.output) || (call.name === "http_request" && failedSource(detail));
    const expected = subject.case_id === "addressed_repair" && expectedRefAbsence(detail, details.get(next?.call_id ?? ""), event.event_id);
    requireProof(!stop || expected, field, "work continued after a terminal refusal or failed required source");
  }
  if (subject.case_id === "truncated_required_log") requireProof(subject.tool_records.some(call => intentionalLogTruncation(records.json(call.record, callSchema, field))), field, "intentional source-truncation notice is missing");
  return outputs;
}

function checkClaims(subject: Case, outputs: Map<string, string>, records: Records, field: string): void {
  const answer = records.text(subject.answer, `${field}.answer`);
  for (const claim of subject.claims) {
    const selected = bytesAt(answer, claim.answer_span, `${field}.claims.answer_span`);
    const witnesses = claim.witnesses.map(witness => {
      let text: string;
      if (witness.kind === "tool_output") {
        const output = outputs.get(witness.call_id);
        requireProof(output !== undefined, field, "claim cites another case or missing tool output");
        text = output;
      } else {
        requireProof(witness.field !== "event_detail" || claim.kind === "identifier", field, "native answer cannot support its own outcome or an external-source claim");
        text = records.text(witness.field === "event_detail" ? subject.event_detail.record : subject[witness.field], field);
        if (witness.field === "event_detail") {
          const event = records.json(subject.event_detail.record, eventSchema, field);
          const value = bytesAt(text, witness.span, field).toString();
          requireProof([event.event_id, event.fleet_id, event.workspace_id, event.status].some(id => value === id || value === JSON.stringify(id)), field, "event witness is not a native identity or status field");
        }
      }
      return bytesAt(text, witness.span, `${field}.claims.witnesses.span`);
    });
    requireProof(claim.kind !== "quote" || witnesses.some(witness => witness.equals(selected)), field, "quoted bytes differ from every witness");
  }
}
