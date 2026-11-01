import { describe, expect, test } from "bun:test";
import { checkDevelopment } from "@ci-evidence/development";
import { gradeBehavior } from "@ci-evidence/grade";
import { EvidenceFixture } from "./fixture.ts";
import { AFTER, BASE, HEAD, LOG_QUOTE, id } from "./facts.ts";

function check(fixture: EvidenceFixture): void {checkDevelopment(gradeBehavior(fixture.report, fixture.manifest));}

describe("adversarial native development proof", () => {
  test.each<[(fixture: EvidenceFixture) => void, string]>([
    [f => {f.report.development = null;}, "development"],
    [f => {f.development().review.grade = "incomplete";}, "development.review"],
    [f => {f.development().case_links.responder = "missing_link";}, "development.case_links"],
    [f => {f.development().source.run_id = 99;}, "development.source.evidence"],
    [f => {f.development().source.attempt = 2;}, "development.source.evidence"],
    [f => {f.development().source.head_sha = HEAD;}, "development.source"],
    [f => {f.development().source.failed_step = "Skipped step";}, "development.source"],
    [f => {f.development().source.jobs_url += "&page=9";}, "development.source"],
    [f => {f.development().source.files[0]!.path = "README.md";}, "development.source"],
    [f => {f.development().source.files[0]!.base_blob = HEAD;}, "development.source"],
    [f => {f.development().source.files[0]!.path = "../secret";}, "development.source"],
    [f => {f.development().pull_request.url = "https://example.test/agentsfleet/linkwarden/pull/9";}, "development.pull_request"],
    [f => {f.development().pull_request.head_sha = BASE;}, "development.pull_request"],
    [f => {f.replaceText(f.development().pull_request.diff, " \n");}, "development.pull_request.diff"],
    [f => {f.development().slack.channel_id = "COTHER";}, "development.slack"],
    [f => {f.development().slack.evidence.pop();}, "development.slack"],
    [f => {f.edit(f.development().slack.evidence[1]!, ["text"], "Different visible answer");}, "development.slack"],
    [f => {f.edit(f.development().slack.evidence[1]!, ["event_id"], id(999));}, "development.slack"],
    [f => {f.replaceText(f.development().regression.before.output, "not executed");}, "development.regression.execution"],
    [f => {f.replaceText(f.development().regression.after.output, f.read(f.development().regression.after.output).replace("3 passed", "2 passed"));}, "development.regression.execution"],
    [f => {f.edit(f.development().regression.after.execution.run.record, ["response", "head_sha"], BASE);}, "development.regression.execution"],
    [f => {f.edit(f.development().regression.after.execution.job.record, ["response", "run_id"], 100);}, "development.regression.execution"],
    [f => {f.development().regression.after.execution.log_url += "/another";}, "development.regression.execution"],
    [f => {f.development().regression.after.counts.skipped = 1;}, "development.regression"],
    [f => {f.replaceText(f.development().regression.fixture.content, "Changed assertions");}, "development.regression.execution"],
    [f => {f.development().regression.fixture.path = "unrelated.test.ts";}, "development.regression.execution"],
    [f => {f.development().runtime.runner_observations[0]!.running_sha256 = "0".repeat(64);}, "development.runtime.runner"],
    [f => {f.development().runtime.source_revision = BASE;}, "development.runtime"],
    [f => {f.development().runtime.checks.runner = "incomplete";}, "development.runtime.checks"],
    [f => {f.edit(f.development().runtime.evidence.deployment, ["jobs", "response", "jobs", 1, "name"], "metal / result");}, "development.runtime.deployment"],
    [f => {f.edit(f.development().runtime.evidence.deployment, ["run", "response", "created_at"], "2026-10-31T23:00:00Z");}, "development.runtime"],
    [f => {f.edit(f.development().runtime.evidence.health, ["observed_at"], "2026-10-31T23:00:00Z");}, "development.runtime"],
    [f => {f.edit(f.development().runtime.evidence.readiness, ["observed_at"], AFTER);}, "development.runtime"],
    [f => {f.edit(f.development().runtime.runner_observations[0]!.probe, ["after", "pid"], 124);}, "development.runtime.runner"],
    [f => {
      const runtime = f.development().runtime, probe = runtime.runner_observations[0]!.probe;
      f.edit(probe, ["artifact", "entry_name"], "archive.zip"); f.edit(runtime.evidence.runner, ["probes", 0], probe);
    }, "development.runtime.runner.probe"],
    [f => {f.development().memory.review.grade = "incomplete";}, "development.memory"],
    [f => {f.edit(f.development().memory.recall_event.record, ["fleet_id"], id(999));}, "development.memory"],
    [f => {f.edit(f.development().memory.entry.record, ["response", "items"], []);}, "development.memory"],
    [f => {f.edit(f.development().memory.entry.record, ["response", "items", 0, "updated_at"], "2026-10-31T23:00:00Z");}, "development.memory"],
    [f => {f.edit(f.development().memory.entry.record, ["observed_at"], "2026-10-31T23:00:00Z");}, "development.memory"],
    [f => {f.edit(f.development().memory.recall_tools[0]!.record, ["response", "arguments", "method"], "POST");}, "development.memory"],
    [f => {f.edit(f.development().memory.recall_tools[0]!.record, ["response", "truncated"], true);}, "development.memory"],
    [f => {
      const memory = f.development().memory;
      memory.recall_call = {...memory.recall_call, record: f.add(f.json(memory.recall_call.record))};
      f.edit(memory.recall_call.record, ["response", "arguments", "query"], "contradictory query");
    }, "development.memory"],
  ])("rejects uncorrelated or contradictory evidence %#", (mutate, field) => {
    const fixture = new EvidenceFixture(); mutate(fixture);
    expect(() => check(fixture)).toThrow(field);
  });

  test("requires fleet-owned metadata instead of an operator's preparation record", () => {
    const fixture = new EvidenceFixture();
    const subject = fixture.subject();
    fixture.edit(subject.tool_records[0]!.record, ["response", "arguments", "url"], "https://api.github.com/other");
    expect(() => check(fixture)).toThrow("development.source");
  });

  test("does not accept an HTML page that puts a text media type in its body", () => {
    const fixture = new EvidenceFixture();
    const log = fixture.subject().tool_records.find(call => call.call_id === "1:8");
    if (!log) throw new Error("synthetic log missing");
    const output = `Status: 200\nContent-Type: text/html\n\nResponse Body:\nContent-Type: text/plain\n${LOG_QUOTE}`;
    fixture.edit(log.record, ["response", "output"], output);
    const witness = fixture.subject().claims[0]!.witnesses[0]; if (!witness) throw new Error("synthetic witness missing");
    witness.span = [Buffer.byteLength(output) - Buffer.byteLength(LOG_QUOTE), Buffer.byteLength(output)];
    expect(() => check(fixture)).toThrow("development.source");
  });

  test("retains legitimate log text that quotes a truncation marker", () => {
    const fixture = new EvidenceFixture();
    const log = fixture.subject().tool_records.find(call => call.call_id === "1:8");
    if (!log) throw new Error("synthetic log missing");
    const value = fixture.json(log.record);
    if (typeof value !== "object" || value === null) throw new Error("synthetic log shape");
    fixture.edit(log.record, ["response", "output"], "Status: 200\nContent-Type: text/plain\n\nResponse Body:\nLog says synthetic failure\n[Response truncated at 123 bytes]");
    expect(() => check(fixture)).not.toThrow();
  });

  test("normalizes Slack's visible URL syntax without accepting a labelled substitute", () => {
    const fixture = new EvidenceFixture();
    const ref = fixture.development().slack.evidence[3]!;
    const answer = fixture.read(fixture.subject(6).answer);
    fixture.edit(ref, ["text"], `<${answer}|${answer}>`);
    expect(() => check(fixture)).not.toThrow();
    fixture.edit(ref, ["text"], `<${answer}|another draft>`);
    expect(() => check(fixture)).toThrow("development.slack");
  });
});
