import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { Writable } from "node:stream";
import { Effect, Schema } from "effect";
import { renderFleet, renderReview } from "../../src/commands/chat/render.ts";
import { makeStdioOutput, OUTPUT_FORMAT } from "../../src/services/output.ts";
import { FLEET, REVIEW } from "./fixture.ts";

const POLICY: unknown = JSON.parse(readFileSync(new URL("../../../tests/fixtures/fleets/goal-repair/decision-policy.json", import.meta.url), "utf8"));
const QUESTION = "Does the patch fix the task's accepted Linkwarden defect?";
const YES = "Current code and regression evidence demonstrate the accepted behavior.";
const NO = "The defect remains, scope changed, or evidence is missing or contradictory.";

describe("reviewed question output", () => {
  test("shows complete questions and criteria separately with the saved revision and preserves JSON output", async () => {
    const review = withPolicy(POLICY);
    const {output, stdout} = streams(OUTPUT_FORMAT.text);
    await Effect.runPromise(renderReview(output, review));
    const displayed = stdout.join("");
    expect(displayed).toContain("Saved setup revision: R7. Saving changes creates a new revision.");
    expect(displayed).toContain("What counts as done?");
    expect(displayed).toContain(`1. ${QUESTION}`);
    expect(displayed).toContain(`Yes: ${YES}`);
    expect(displayed).toContain(`No: ${NO}`);
    expect(displayed).not.toContain("Target association:");
    expect(displayed).not.toContain("Message routing questions");
    expect(displayed).not.toContain("Noul");
    expect(displayed).not.toContain("defect-fixed");
    expect(displayed).toContain("Work already started keeps its saved success criteria.");
    expect(displayed.split(QUESTION)).toHaveLength(2);
    expect(displayed).not.toContain('"decision_policy"');
    const json = streams(OUTPUT_FORMAT.json);
    await Effect.runPromise(renderReview(json.output, review));
    expect(json.stdout.join("").trim().split("\n")).toHaveLength(1);
    expect(JSON.parse(json.stdout.join(""))).toEqual(review);
  });

  test("keeps incomplete policy unknown rather than presenting partial criteria or silent defaults", async () => {
    const fixture = Schema.decodeUnknownSync(Schema.Struct({routing: Schema.Unknown,
      completion: Schema.Record(Schema.String, Schema.Unknown)}))(POLICY);
    for (const policy of [null, {}, {routing: {}}, {completion: {statements: []}},
      {...fixture, unexpected_policy: true},
      {...fixture, completion: {...fixture.completion, statements: []}},
      {...fixture, completion: {...fixture.completion, next_action: {question: "Choose", criteria: {
        CONTINUE: "Go", WAIT: "Wait", DONE: "Done", BLOCKED: "Blocked", PUBLISH: "Any target",
      }}}},
    ]) {
      const {output, stdout} = streams(OUTPUT_FORMAT.text);
      await Effect.runPromise(renderReview(output, withPolicy(policy)));
      expect(stdout.join("")).toContain("Success criteria could not be read.");
      expect(stdout.join("")).not.toContain("What counts as done?");
      expect(stdout.join("")).not.toContain(QUESTION);
    }
  });

  test("distinguishes an ordinary setup from unreadable policy and shows the saved fleet revision", async () => {
    const {output, stdout} = streams(OUTPUT_FORMAT.text);
    const ordinary = {...REVIEW, effective: {...REVIEW.effective, definition: {...REVIEW.effective.definition,
      config: {name: "argo", "x-agentsfleet": {tools: [], budget: {daily_nanos: "1000000000"}}}}}};
    await Effect.runPromise(renderReview(output, ordinary));
    await Effect.runPromise(renderFleet(output, {...FLEET, setup_revision: "12"}));
    expect(stdout.join("")).not.toContain("What counts as done?");
    expect(stdout.join("")).not.toContain("Success criteria could not be read.");
    expect(stdout.join("")).toContain("Saved setup revision: R12");
  });

  test("preserves Unicode questions within the byte ceiling and refuses oversized private wording", async () => {
    const fixture = Schema.decodeUnknownSync(Schema.Struct({routing: Schema.Unknown, completion: Schema.Struct({
      statements: Schema.Array(Schema.Record(Schema.String, Schema.Unknown)), next_action: Schema.Unknown,
    })}))(POLICY);
    const valid = "🙂".repeat(120);
    const invalid = "界".repeat(200);
    for (const text of [valid, invalid]) {
      const policy = {...fixture, completion: {...fixture.completion,
        statements: fixture.completion.statements.map((statement, index) => index === 0 ? {...statement, question: text} : statement)}};
      const {output, stdout} = streams(OUTPUT_FORMAT.text);
      await Effect.runPromise(renderReview(output, withPolicy(policy)));
      if (text === valid) {
        expect(stdout.join("")).toContain(text);
        expect(stdout.join("")).toContain(`Yes: ${YES}`);
      } else {
        expect(stdout.join("")).toContain("Success criteria could not be read.");
        expect(stdout.join("")).not.toContain(text);
      }
    }
  });
});

function withPolicy(policy: unknown) {
  return {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "7",
    definition: {...REVIEW.effective.definition, config: {name: "linkwarden-repair",
      "x-agentsfleet": {tools: [], budget: {daily_nanos: "1000000000"}, decision_policy: policy}}}}};
}

function streams(format: typeof OUTPUT_FORMAT[keyof typeof OUTPUT_FORMAT]) {
  const stdout: string[] = [];
  return {stdout, output: makeStdioOutput({format,
    stdout: new Writable({write(value, _encoding, done) {stdout.push(String(value)); done();}}),
    stderr: new Writable({write(_value, _encoding, done) {done();}})})};
}
