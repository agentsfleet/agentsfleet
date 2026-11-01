import { describe, expect, test } from "bun:test";
import { EvidenceError, Records, bytesAt, canonical, hash } from "@ci-evidence/records";
import { gradeBehavior } from "@ci-evidence/grade";
import { checkDevelopment } from "@ci-evidence/development";
import { EvidenceFixture } from "./ci-evidence/fixture.ts";
import { AFTER, HEAD, id } from "./ci-evidence/facts.ts";

describe("closed offline CI evidence", () => {
  test("the complete synthetic positive passes all eight cases and development joins", () => {
    const fixture = new EvidenceFixture();
    const grade = gradeBehavior(fixture.report, fixture.manifest);
    expect({grounding: grade.grounding, behavior: grade.behavior}).toEqual({grounding: 8, behavior: 8});
    expect(() => checkDevelopment(grade)).not.toThrow();
  });

  test.each<[(fixture: EvidenceFixture) => void, string]>([
    [f => {f.report.cases.pop();}, "report"],
    [f => {f.report.cases[1] = f.subject();}, "cases"],
    [f => {f.report.cases[1] = {...f.subject(1), event_id: f.subject().event_id};}, "cases"],
    [f => {f.manifest.cases[1] = {...f.manifest.cases[0]!}; f.report.corpus_sha256 = hash(canonical(f.manifest));}, "manifest.cases"],
    [f => {f.report.corpus_sha256 = "0".repeat(64);}, "corpus_sha256"],
    [f => {f.subject().variant_id = "unreviewed";}, "cases[0]"],
    [f => {f.subject().outcome = "draft";}, "cases[0]"],
    [f => {f.report.records.push({...f.report.records[0]!});}, "records"],
    [f => {f.subject().answer.path = "records/missing.txt";}, "cases[0]"],
    [f => {f.subject().answer.sha256 = "0".repeat(64);}, "cases[0]"],
    [f => {f.edit(f.subject().setup_after, ["setup_revision"], "2");}, "cases[0]"],
    [f => {f.subject().setup_revision = "2";}, "cases[0]"],
    [f => {f.replaceText(f.subject().setup_before, "{");}, "cases[0].setup_before"],
    [f => {f.report.instructions_sha256.responder = "0".repeat(64);}, "cases[0]"],
    [f => {f.edit(f.subject().review_snapshot, ["review_digest"], `authoring:v1:sha256:${"0".repeat(64)}`);}, "cases[0]"],
    [f => {f.edit(f.subject().review_snapshot, ["effective", "definition", "instructions_md"], "Different review");}, "cases[0]"],
    [f => {f.edit(f.subject().review_snapshot, ["effective", "runtime_model_ref"], "another-ref");}, "cases[0]"],
    [f => {f.edit(f.subject().setup_receipt, ["response", "id"], id(999));}, "cases[0]"],
    [f => {f.edit(f.subject().setup_receipt, ["request"], {review_digest: f.subject().review_digest});}, "cases[0]"],
    [f => {f.edit(f.subject().setup_receipt, ["read_url"], "https://other.example.test/fleets");}, "cases[0]"],
    [f => {f.subject().event_detail.read_url += "/other";}, "cases[0]"],
    [f => {f.edit(f.subject().event_detail.record, ["fleet_id"], id(999));}, "cases[0]"],
    [f => {f.edit(f.subject().event_detail.record, ["response_text"], "A substituted answer");}, "cases[0]"],
    [f => {f.edit(f.subject().event_detail.record, ["request_json"], "{");}, "cases[0].event_detail.request_json"],
    [f => {f.edit(f.subject().event_detail.record, ["request_json"], JSON.stringify({message: "Different prompt", channel_id: "CFIXTURE", reply_thread_ts: "123.456"}));}, "cases[0]"],
    [f => {f.subject().runtime_revision = HEAD;}, "cases[0]"],
    [f => {f.edit(f.subject().event_detail.record, ["tool_calls"], null);}, "cases[0]"],
    [f => {f.edit(f.subject().event_detail.record, ["tool_calls", "omitted_call_count"], 1);}, "cases[0]"],
    [f => {f.subject().tool_records.pop();}, "cases[0]"],
    [f => {f.subject().tool_records[0]!.call_id = "another-case-call";}, "cases[0]"],
    [f => {f.edit(f.subject().tool_records[0]!.record, ["read_url"], "https://other.example.test/call");}, "cases[0]"],
    [f => {f.edit(f.subject().tool_records[0]!.record, ["response", "truncated_arguments"], true);}, "cases[0]"],
    [f => {f.edit(f.subject().tool_records[0]!.record, ["response", "truncated"], true);}, "cases[0]"],
    [f => {f.subject().claims[0]!.witnesses = [{kind: "tool_output", call_id: "another-case", span: [0, 3]}];}, "cases[0]"],
    [f => {f.subject().claims[0]!.answer_span = [0, 1_000_000];}, "cases[0].claims.answer_span"],
    [f => {f.subject().claims[0]!.witnesses[0]!.span = [0, 1];}, "cases[0]"],
    [f => {f.subject(1).claims[0]!.witnesses = [{kind: "case_record", field: "event_detail", span: [0, 2]}];}, "cases[1]"],
    [f => {f.subject().review.at = "2026-01-01T00:00:00Z";}, "cases[0]"],
    [f => {f.edit(f.subject().model_freeze, ["evaluation_id"], "older-campaign");}, "cases[0]"],
    [f => {f.edit(f.subject().model_freeze, ["agreed_at"], AFTER);}, "cases[0]"],
    [f => {f.edit(f.subject().settings_before, ["observed_at"], AFTER);}, "cases[0]"],
    [f => {f.edit(f.subject().settings_after, ["selection", "read_url"], "https://other.example.test/models");}, "cases[0]"],
    [f => {f.edit(f.subject().settings_before, ["selection", "response", "items"], []);}, "cases[0]"],
    [f => {f.edit(f.subject().settings_before, ["catalogue", "response", "models"], []);}, "cases[0]"],
    [f => {f.edit(f.subject().model_freeze, ["model_record_id"], id(999));}, "cases[0]"],
    [f => {f.report.model.settings.inference.max_output_tokens = 129;}, "cases[0]"],
    [f => {f.report.model.settings.context_cap_tokens = 8192;}, "cases[0]"],
    [f => {f.report.model.settings.context = {different: true};}, "cases[0]"],
    [f => {f.report.model.version = "unavailable-version";}, "cases[0]"],
    [f => {f.edit(f.subject().settings_after, ["catalogue", "response", "models", 0, "metadata", "label"], "drift");}, "cases[0]"],
    [f => {f.edit(f.subject().model_identity, ["event_id"], id(999));}, "cases[0]"],
    [f => {f.edit(f.subject().model_identity, ["captured_at"], "2026-01-01T00:00:00Z");}, "cases[0]"],
    [f => {f.edit(f.subject().model_identity, ["leases", 0, "model"], "another-model");}, "cases[0]"],
    [f => {f.edit(f.subject().model_identity, ["leases", 0, "status"], "active");}, "cases[0]"],
  ])("rejects invalid native proof %#", (mutate, field) => {
    const fixture = new EvidenceFixture(); mutate(fixture);
    expect(() => gradeBehavior(fixture.report, fixture.manifest)).toThrow(field);
  });

  test("failed and incomplete reviews stay in the fixed eight-case denominator", () => {
    const fixture = new EvidenceFixture();
    fixture.subject().review.stopping_scope = "fail";
    fixture.subject(1).review.grounding = "incomplete";
    fixture.subject(2).review.usefulness = "fail";
    fixture.subject(3).review.all_claims_reviewed = false;
    fixture.subject(4).review.findings = ["Continued after source refusal"];
    const grade = gradeBehavior(fixture.report, fixture.manifest);
    expect(grade.grounding).toBe(4); expect(grade.behavior).toBe(3);
  });

  test("UTF-8 byte spans preserve emoji and canonical JSON uses UTF-16 key order", () => {
    expect(bytesAt("a💬z", [1, 5], "span").toString()).toBe("💬");
    expect(() => bytesAt("a💬z", [1, 4], "span")).toThrow("splits UTF-8");
    expect(() => bytesAt("abc", [2, 2], "span")).toThrow("empty");
    expect(canonical({"\ufffd": 1, "💬": [true, null, 1e30]})).toBe('{"💬":[true,null,1e+30],"�":1}');
    for (const invalid of [undefined, Infinity, NaN, "\ud800"]) expect(() => canonical(invalid)).toThrow(EvidenceError);
    expect(() => new Records([{name: "a", text: "x"}, {name: "a", text: "y"}])).toThrow("duplicate");
  });
});
