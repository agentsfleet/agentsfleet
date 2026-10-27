import { expect, it } from "vitest";
import { createDraftThreadProjection, draftThreadMessages } from "@/lib/api/draft-thread-messages";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { UNKNOWN_USAGE_REASON } from "@/lib/api/draft-recovery";
import { ACTIVITY_KIND, FLEET_ACTIVITY } from "@/lib/events/fleet-activity";
import { DRAFT, MESSAGE } from "./helpers/draft-fixtures";

const HISTORY_COUNTS = [100, 1_000];

// Value: protects=receipts keep their timeline position while an optimistic answer has no durable instant;
// fails_when=undated pending answers sort ahead of recorded activity; seam=none; no_claim=server event ordering.
it("places a dated receipt before the undated answer to an optimistic request", () => {
  const operation = MESSAGE.operation_id;
  const rows = draftThreadMessages({...EMPTY_DRAFT_VIEW, draft: DRAFT,
    optimistic: {operation, text: "Another request", createdAt: MESSAGE.created_at},
    activities: [{event_id: "saved", event_type: FLEET_ACTIVITY.type, status: FLEET_ACTIVITY.status,
      created_at: MESSAGE.created_at, request_json: JSON.stringify({kind: ACTIVITY_KIND.setupSaved, revision: "1"})}]});
  expect(rows.map(row => row.id)).toEqual([`pending:${operation}`, "activity:saved", `pending-answer:${operation}`]);
  expect(rows[1]).toMatchObject({role: "system", content: [{type: "text", text: "Save done v1"}]});
  expect(rows[2]).toMatchObject({role: "assistant", status: {type: "running"}});
});

it.each(["deadline", "cancelled", "provider_interrupted", "completion_unverified"] as const)("preserves the recorded %s in failure details without replacing a partial answer", (cause) => {
  const failure = {reason: UNKNOWN_USAGE_REASON, cause, error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"};
  const rows = draftThreadMessages({...EMPTY_DRAFT_VIEW, draft: DRAFT,
    messages: [MESSAGE, {...MESSAGE, role: "assistant", state: "failed", text: "Partial answer", failure}]});
  expect(rows[1]).toMatchObject({content: [{type: "text", text: "Partial answer"}], status: {type: "complete"},
    metadata: {custom: {failure, retryText: MESSAGE.text}}});
});

it("preserves unsettled usage after stopping without inventing a cause or retry text", () => {
  const failure = {reason: UNKNOWN_USAGE_REASON, error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"};
  const rows = draftThreadMessages({...EMPTY_DRAFT_VIEW, draft: {...DRAFT, cancelled_through: MESSAGE.sequence},
    messages: [{...MESSAGE, role: "assistant", state: "failed", text: undefined, failure}]});
  expect(rows).toMatchObject([{content: [{type: "text", text: ""}], metadata: {custom: {failure, retryText: undefined}}}]);
});

it("keeps settled messages stable while another answer streams and keeps conversation caches separate", () => {
  const project = createDraftThreadProjection();
  const messages = [MESSAGE, {...MESSAGE, role: "assistant" as const, text: "Done"},
    {...MESSAGE, sequence: "2", state: "running" as const}];
  const view = {...EMPTY_DRAFT_VIEW, draft: DRAFT, messages};
  const first = project(view);
  const next = project({...view, liveReply: {sequence: "2", stream: "stream", text: "Growing", gapped: false}});
  expect(next[0]).toBe(first[0]);
  expect(next[1]).toBe(first[1]);
  expect(next[3]).toMatchObject({content: [{text: "Growing"}]});
  expect(createDraftThreadProjection()(view)[0]).not.toBe(first[0]);
});

it("shows matching live text while running and preserves the durable answer after completion", () => {
  const liveReply = {sequence: MESSAGE.sequence, stream: "private-stream", text: "Partial answer", gapped: false};
  const view = {...EMPTY_DRAFT_VIEW, draft: DRAFT, liveReply};
  expect(draftThreadMessages({...view, messages: [{...MESSAGE, state: "running"}]})).toMatchObject([
    {role: "user", content: [{text: MESSAGE.text}]},
    {role: "assistant", content: [{text: liveReply.text}], status: {type: "running"}},
  ]);
  expect(draftThreadMessages({...view, messages: [{...MESSAGE, role: "assistant", text: "Durable answer"}]}))
    .toMatchObject([{content: [{text: "Durable answer"}], status: {type: "complete"}}]);
});

it("does not invent retry text for a user row without recorded request text", () => {
  const failure = {reason: "provider_failed", error_code: "UZ-AUTHOR-014", recovery_action: "retry_message"};
  const rows = draftThreadMessages({...EMPTY_DRAFT_VIEW, draft: DRAFT,
    messages: [{...MESSAGE, text: undefined}, {...MESSAGE, role: "assistant", state: "failed", text: "Partial answer", failure}]});
  expect(rows[1]).toMatchObject({content: [{text: "Partial answer"}], metadata: {custom: {failure, retryText: undefined}}});
});

it("invalidates a cached failure when its original request becomes available", () => {
  const failure = {reason: "provider_failed", error_code: "UZ-AUTHOR-014", recovery_action: "retry_message"};
  const answer = {...MESSAGE, role: "assistant" as const, state: "failed" as const, failure};
  const project = createDraftThreadProjection();
  const before = project({...EMPTY_DRAFT_VIEW, draft: DRAFT, messages: [answer]});
  const after = project({...EMPTY_DRAFT_VIEW, draft: DRAFT, messages: [MESSAGE, answer]});
  expect(after[1]).not.toBe(before[0]);
  expect(after[1]).toMatchObject({metadata: {custom: {retryText: MESSAGE.text}}});
});

it.each(HISTORY_COUNTS)("projects %s turns with linear history reads and stable settled rows", (count) => {
  let reads = 0;
  const messages = Array.from({length: count}, (_, index) => [
    {...MESSAGE, get sequence() { reads++; return String(index); }},
    {...MESSAGE, role: "assistant" as const, get sequence() { reads++; return String(index); }, text: "Done"},
  ]).flat();
  const project = createDraftThreadProjection();
  const view = {...EMPTY_DRAFT_VIEW, draft: DRAFT, messages};
  const first = project(view);
  expect(reads).toBeLessThanOrEqual(count * 20);
  reads = 0;
  const next = project({...view, liveReply: {sequence: String(count), stream: "stream", text: "Growing", gapped: false}});
  expect(reads).toBeLessThanOrEqual(count * 20);
  expect(next.every((row, index) => row === first[index])).toBe(true);
});
