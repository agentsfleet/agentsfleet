import { describe, expect, test } from "bun:test";
import { eventSchema } from "@ci-evidence/native";
import { checkCalls } from "@ci-evidence/cases";
import { gradeBehavior } from "@ci-evidence/grade";
import { httpEnvelope } from "@ci-evidence/sources";
import { Records } from "@ci-evidence/records";
import { EvidenceFixture } from "./fixture.ts";

describe("native source boundaries", () => {
  test.each(["1:0", "1:9223372036854775808", "9223372036854775808:1", "-9223372036854775809:1"])("rejects non-native call identity %s", value => {
    const fixture = new EvidenceFixture();
    fixture.edit(fixture.subject().event_detail.record, ["tool_calls", "calls", 0, "call_id"], value);
    expect(() => gradeBehavior(fixture.report, fixture.manifest)).toThrow("cases[0].event_detail");
  });

  test("never combines calls from different lease attempts into one event trace", () => {
    const fixture = new EvidenceFixture();
    fixture.edit(fixture.subject().event_detail.record, ["tool_calls", "calls", 1, "call_id"], "2:2");
    expect(() => gradeBehavior(fixture.report, fixture.manifest)).toThrow("mixes lease fences");
  });
  test.each(["file_read", "http_request", "memory_store"])("refuses %s after a policy denial", name => {
    const fixture = new EvidenceFixture(), subject = fixture.subject(4);
    const denied = fixture.call(subject, "1:1", "http_request", {url: "https://api.github.com/denied"}, "[request_policy_not_allowed] Required source unavailable", "failed");
    const next = fixture.call(subject, "1:2", name, {url: "https://api.github.com/other"}, "Unauthorized follow-up");
    subject.tool_records = [denied.detail, next.detail];
    fixture.edit(subject.event_detail.record, ["tool_calls", "calls"], [denied.inventory, next.inventory]);
    expect(() => gradeBehavior(fixture.report, fixture.manifest)).toThrow("work continued after");
  });

  test("binds a full call's native identifier to the addressed inventory", () => {
    const fixture = new EvidenceFixture(); fixture.edit(fixture.subject().tool_records[0]!.record, ["response", "call_id"], "2:99");
    expect(() => gradeBehavior(fixture.report, fixture.manifest)).toThrow("tool identity or address differs");
  });

  test("allows capture truncation only for the intentional truncated required log", () => {
    const fixture = new EvidenceFixture();
    fixture.edit(fixture.subject(4).tool_records[0]!.record, ["response", "truncated"], true);
    expect(() => gradeBehavior(fixture.report, fixture.manifest)).toThrow("required tool output is incomplete");
    const intended = new EvidenceFixture();
    intended.edit(intended.subject(5).tool_records[0]!.record, ["response", "truncated"], true);
    expect(() => gradeBehavior(intended.report, intended.manifest)).not.toThrow();
  });

  test("does not let an outcome claim cite its own native answer", () => {
    const fixture = new EvidenceFixture(), subject = fixture.subject();
    subject.claims.push({kind: "outcome", answer_span: [0, 4], witnesses: [{kind: "case_record", field: "event_detail", span: [0, 4]}]});
    expect(() => gradeBehavior(fixture.report, fixture.manifest)).toThrow("native answer cannot support its own outcome");
  });

  test("allows only real native identity fields as event-record witnesses", () => {
    const fixture = new EvidenceFixture(), subject = fixture.subject();
    const answer = fixture.read(subject.answer), offset = Buffer.byteLength(answer) + 1;
    fixture.replaceText(subject.answer, `${answer}\n${subject.event_id}`);
    fixture.edit(subject.event_detail.record, ["response_text"], fixture.read(subject.answer));
    const text = fixture.read(subject.event_detail.record), start = text.indexOf(subject.event_id);
    const claim = {kind: "identifier" as const, answer_span: [offset, offset + subject.event_id.length] as [number, number],
      witnesses: [{kind: "case_record" as const, field: "event_detail" as const, span: [start, start + subject.event_id.length] as [number, number]}]};
    subject.claims.push(claim);
    expect(() => gradeBehavior(fixture.report, fixture.manifest)).not.toThrow();
    claim.witnesses[0]!.span = [0, 4];
    expect(() => gradeBehavior(fixture.report, fixture.manifest)).toThrow("not a native identity or status field");
  });

  test("recognizes exact-ref absence only with the immediate complete empty all-state reconciliation", () => {
    const fixture = new EvidenceFixture(), subject = fixture.subject(6);
    const branch = `agentsfleet-repair/${Buffer.from(subject.event_id).toString("base64url")}`;
    const prefix = "https://api.github.com/repos/agentsfleet/linkwarden";
    const missing = fixture.call(subject, "1:1", "http_request", {url: `${prefix}/git/ref/heads/${branch}`}, "[upstream_status] Status: 404\n\nResponse Body:\n{}", "failed");
    const reconcile = fixture.call(subject, "1:2", "http_request", {url: `${prefix}/pulls?state=all&base=dev&head=agentsfleet:${branch}`}, "Status: 200\n\nResponse Body:\n[\n\n]");
    subject.tool_records = [missing.detail, reconcile.detail];
    fixture.edit(subject.event_detail.record, ["tool_calls", "calls"], [missing.inventory, reconcile.inventory]);
    const check = () => checkCalls(subject, eventSchema.parse(fixture.json(subject.event_detail.record)), new Records(fixture.report.records), subject.event_detail.read_url, "reconcile");
    expect(() => check()).not.toThrow();
    fixture.edit(reconcile.detail.record, ["response", "output"], "[upstream_status] Status: 403\n\nResponse Body:\n{}");
    expect(check).toThrow("work continued after");
  });

  test("checks media types and machine truncation markers before the response body", () => {
    const actual = httpEnvelope("Status: 200\nContent-Type: text/plain; charset=utf-8\n\nResponse Body:\n[Response truncated at 123 bytes]");
    expect(actual).toMatchObject({status: 200, plain: true, truncated: false});
    expect(httpEnvelope("Status: 200\nContent-Type: text/html\n\nResponse Body:\nContent-Type: text/plain").plain).toBe(false);
  });
});
