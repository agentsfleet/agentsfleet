import { describe, expect, test } from "bun:test";
import { Effect, Redacted } from "effect";
import { AttemptOutput } from "../src/commands/fleet/attempt-output.ts";
import { openTurnTail } from "../src/commands/fleet/steer-turns.ts";
import { ActivityPrinter } from "../src/commands/fleet/task-activity.ts";
import { ToolActivityPrinter } from "../src/commands/fleet/tool-activity.ts";
import { TURN_ID, FLEET_ID, OTHER_TURN_ID, TOKEN, WS_ID, eventStream, makeLayer, makeRecorder } from "./helpers-fleet-steer.ts";

describe("attempt output ownership", () => {
  test("prints one restart line for its own event and attempt", async () => {
    const rec = makeRecorder();
    const frame = (type: string, fence: string, text = "", event = TURN_ID) => ({id: null, type,
      data: {turn_id: event, fencing_token: fence, text, text_kind: "answer"}});
    const handle = await Effect.runPromise(openTurnTail(WS_ID, FLEET_ID, Redacted.make(TOKEN), eventStream([
      frame("chunk", "9007199254740992", "old draft"),
      frame("run_restarted", "9007199254740993", "", OTHER_TURN_ID),
      // Lost hint: the first greater output must separate attempts itself.
      frame("chunk", "9007199254740993", "current draft"),
      frame("run_restarted", "9007199254740993"),
      frame("run_restarted", "9007199254740992"),
      frame("chunk", "9007199254740992", "STALE"),
      {id: null, type: "turn_complete", data: {turn_id: TURN_ID, status: "processed", final_reply: "saved answer"}},
    ])).pipe(Effect.provide(makeLayer(rec))));
    handle.deliverTurnId(TURN_ID);
    expect((await handle.awaitOutcome()).kind).toBe("complete");
    const output = rec.stdout.join("\n");
    expect(output.split("Run restarted")).toHaveLength(2);
    expect(output).toContain("old draft");
    expect(output).toContain("current draft");
    expect(output).toContain("saved answer");
    expect(output).not.toContain("STALE");
    await handle.close();
  });

  test("initializes silently, marks a first hint, and cannot reopen after completion", () => {
    const lines: string[] = [];
    let resets = 0;
    const output = new AttemptOutput(line => lines.push(line), () => { resets += 1; });
    expect(output.accept("chunk", {fencing_token: "17"})).toBe(true);
    expect(lines).toEqual([]);
    expect(output.accept("run_restarted", {fencing_token: "18"})).toBe(false);
    expect(output.accept("run_restarted", {fencing_token: "18"})).toBe(false);
    expect(output.accept("context_progress", {fencing_token: "17"})).toBe(false);
    expect(resets).toBe(1);
    expect(output.accept("turn_complete", {})).toBe(true);
    expect(output.accept("run_restarted", {fencing_token: "19"})).toBe(false);
    expect(resets).toBe(1);
    const explicit = new AttemptOutput(line => lines.push(line), () => { resets += 1; });
    expect(explicit.accept("run_restarted", {fencing_token: "1"})).toBe(false);
    expect(resets).toBe(2);
  });

  test.each([undefined, 2, "", "0", "01", "-1", "+1", "1e2", "1.0", "9223372036854775808", "99999999999999999999"])(
    "rejects missing or malformed fence %s", fence => {
      const output = new AttemptOutput(() => {}, () => {});
      expect(output.accept("chunk", {fencing_token: fence})).toBe(false);
    });

  test("orders canonical fences across digit lengths up to the database maximum", () => {
    let resets = 0;
    const output = new AttemptOutput(() => {}, () => { resets += 1; });
    expect(output.accept("hello", {})).toBe(true);
    expect(output.accept("chunk", {fencing_token: "9"})).toBe(true);
    expect(output.accept("chunk", {fencing_token: "10"})).toBe(true);
    expect(output.accept("chunk", {fencing_token: "9"})).toBe(false);
    expect(output.accept("chunk", {fencing_token: "9223372036854775807"})).toBe(true);
    expect(output.accept("chunk", {fencing_token: "10"})).toBe(false);
    expect(resets).toBe(2);
  });

  test("resets both presentation caches for the next attempt", () => {
    const lines: string[] = [];
    const activity = new ActivityPrinter(line => lines.push(line));
    const tools = new ToolActivityPrinter(line => lines.push(line));
    const progress = {activity_id: "019a8b12-3456-7890-8abc-0123456789ab", operation: "capture", phase: "saved",
      provider: null, model: null, completed: 1, total: 1};
    const tool = {name: "shell", call_id: "1:0", status: "succeeded", ms: 1};
    activity.frame("task_transfer_progress", {progress});
    tools.frame("tool_call_completed", tool);
    activity.frame("task_transfer_progress", {progress});
    tools.frame("tool_call_completed", tool);
    expect(lines).toHaveLength(2);
    activity.reset();
    tools.reset();
    activity.frame("task_transfer_progress", {progress});
    tools.frame("tool_call_completed", tool);
    expect(lines).toHaveLength(4);
  });
});
