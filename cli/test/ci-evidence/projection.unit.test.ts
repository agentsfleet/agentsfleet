import { describe, expect, test } from "bun:test";
import { argumentProjection, checkProjection } from "@ci-evidence/projection";
import { gradeBehavior } from "@ci-evidence/grade";
import { checkDevelopment } from "@ci-evidence/development";
import { EvidenceFixture } from "./fixture.ts";

describe("native trace consistency", () => {
  test("a successful replacement detail cannot hide the inventory's recorded denial", () => {
    const fixture = new EvidenceFixture();
    fixture.edit(fixture.subject().event_detail.record, ["tool_calls", "calls", 0, "output_head"], "[request_policy_not_allowed] denied");
    expect(() => gradeBehavior(fixture.report, fixture.manifest)).toThrow("output head contradicts");
  });

  test("compares native counts, retained output edges and bounded arguments", () => {
    const detail = {call_id: "1:1", arguments: {path: "a.ts"}, output: "first\nlast", output_line_count: 2,
      truncated: false, truncated_arguments: false};
    const inventory = {call_id: "1:1", name: "file_read", status: "succeeded" as const,
      arguments: {path: "a.ts"}, output_head: "first\n", output_tail: "last", output_line_count: 2};
    expect(() => checkProjection(inventory, detail, "call")).not.toThrow();
    expect(() => checkProjection({...inventory, output_line_count: 3}, detail, "call")).toThrow("counts contradict");
    expect(() => checkProjection({...inventory, output_tail: "another"}, detail, "call")).toThrow("tail contradicts");
    expect(() => checkProjection({...inventory, arguments: {path: "b.ts"}}, detail, "call")).toThrow("arguments contradict");
    expect(() => checkProjection(inventory, {...detail, output: "first\n", truncated: true}, "call")).not.toThrow();
  });

  test("matches native byte clipping without breaking UTF-8 or inventing retained arguments", () => {
    expect(argumentProjection({path: "a".repeat(255) + "é"})).toEqual({path: "a".repeat(255)});
    expect(argumentProjection({array: [null, true, 3, {nested: "value"}]})).toEqual({array: [null, true, 3, {nested: "value"}]});
    expect(argumentProjection(Array.from({length: 3}, () => "value"))).toEqual({});
    expect(argumentProjection(Object.fromEntries(Array.from({length: 20}, (_, index) => [`key${index}`, "x".repeat(256)])))).toEqual({});
  });

  test.each(["memory_store", "http_request"])("all required evidence precedes %s", name => {
    const fixture = new EvidenceFixture(), subject = fixture.subject(name === "memory_store" ? 0 : 6);
    const calls = subject.tool_records.map(call => call.call_id);
    const last = calls.length - 1;
    const inventory = fixture.json(subject.event_detail.record);
    if (typeof inventory !== "object" || inventory === null) throw new Error("fixture event missing");
    const trace = Reflect.get(inventory, "tool_calls");
    if (typeof trace !== "object" || trace === null) throw new Error("fixture trace missing");
    const rows: unknown = Reflect.get(trace, "calls");
    if (!Array.isArray(rows)) throw new Error("fixture calls missing");
    const mutation = rows.splice(last, 1); rows.splice(last - 1, 0, ...mutation);
    fixture.edit(subject.event_detail.record, ["tool_calls", "calls"], rows);
    expect(() => checkDevelopment(gradeBehavior(fixture.report, fixture.manifest))).toThrow("development.source");
  });

  test("contradictory selected-job listing cannot avoid the selected log", () => {
    const fixture = new EvidenceFixture();
    const call = fixture.subject().tool_records.find(call => call.call_id === "1:3");
    if (!call) throw new Error("fixture job listing missing");
    fixture.edit(call.record, ["response", "output"], 'Status: 200\n\nResponse Body:\n{"total_count":1,"jobs":[{"id":88,"run_id":77,"run_attempt":1,"conclusion":"success","steps":[]}]}');
    expect(() => checkDevelopment(gradeBehavior(fixture.report, fixture.manifest))).toThrow("contradictory");
  });
});
