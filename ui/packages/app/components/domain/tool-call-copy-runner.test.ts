import { describe, expect, it } from "vitest";

import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { CELL_STATE, toolCopy, verbFor } from "./tool-call-copy";
import { patchDiff } from "./tool-call-copy-runner";
import { DIFF_ROW } from "./tool-call-diff";
import { ARGS_NOT_RECORDED, PLAN_STATUS, TOOL_BODY, TOOL_NAME } from "./tool-call-shape";
import { ARGS_LEAF_MAX_BYTES, CLIP_MARK } from "./tool-call-text";

// Pin tests throughout: the literals are the words each runner tool's cell
// draws, from argument names the runner and its specs give.

function header(name: string, args: ToolArgs | undefined, state: (typeof CELL_STATE)[keyof typeof CELL_STATE]): string {
  const copy = toolCopy(name, args);
  return `${verbFor(copy.verbs, state)} ${copy.target}`.trim();
}

const PATCH = [
  "*** Begin Patch",
  "*** Update File: /workspace/src/app.ts",
  "@@ function main",
  " keep",
  "-old",
  "+new",
  "+more",
  "*** Add File: notes.md",
  "+hello",
  "*** End Patch",
].join("\n");

describe("runner tool copy", () => {
  it("test_runner_tools_name_verb_and_target", () => {
    const cases: ReadonlyArray<readonly [string, ToolArgs, string, string]> = [
      [TOOL_NAME.PUSHOVER, { message: "Deploy done", title: "Prod" }, "Notifying Prod", "Notified Prod"],
      [TOOL_NAME.PUSHOVER, { message: "Deploy done\nall green" }, `Notifying Deploy done${CLIP_MARK}`, `Notified Deploy done${CLIP_MARK}`],
      [TOOL_NAME.IMAGE, { path: "/workspace/chart.png" }, "Viewing image chart.png", "Viewed image chart.png"],
      [TOOL_NAME.BROWSER_OPEN, { url: "https://example.test" }, "Opening https://example.test", "Opened https://example.test"],
      [TOOL_NAME.BROWSER, { action: "click", selector: "#save" }, "Clicking #save", "Clicked #save"],
      [TOOL_NAME.BROWSER, { action: "type", selector: "#q", text: "hi" }, "Typing into #q", "Typed into #q"],
      [TOOL_NAME.BROWSER, { action: "text" }, "Reading page", "Read page"],
      [TOOL_NAME.BROWSER, { action: "wait", selector: ".ready" }, "Waiting for .ready", "Waited for .ready"],
      [TOOL_NAME.BROWSER, { action: "scroll" }, "Browsing scroll", "Browsed scroll"],
      [TOOL_NAME.SCREENSHOT, {}, "Taking screenshot", "Took screenshot"],
      [TOOL_NAME.GIT, { args: ["status", "--short"] }, "Running git status --short", "Ran git status --short"],
      // A word that is not a string still shows, rather than vanishing from the command.
      [TOOL_NAME.GIT, { args: ["log", "-n", 5] }, "Running git log -n 5", "Ran git log -n 5"],
      [TOOL_NAME.GIT, { args: "push --force" }, 'Running ({"args":"push --force"})', 'Ran ({"args":"push --force"})'],
      [TOOL_NAME.WRITE_STDIN, { session_id: 3, chars: "y\n" }, 'Writing to terminal 3: "y\\n"', 'Wrote to terminal 3: "y\\n"'],
      // Enter and Ctrl-C, the keys a session mostly gets, drawn so they show.
      [TOOL_NAME.WRITE_STDIN, { session_id: 3, chars: "\n" }, 'Writing to terminal 3: "\\n"', 'Wrote to terminal 3: "\\n"'],
      [TOOL_NAME.WRITE_STDIN, { session_id: 3, chars: "\u0003" }, 'Writing to terminal 3: "\\u0003"', 'Wrote to terminal 3: "\\u0003"'],
      [TOOL_NAME.WRITE_STDIN, { session_id: 3 }, "Waiting for terminal 3", "Waited for terminal 3"],
      [TOOL_NAME.DELEGATE, { task: "Summarise the logs" }, "Delegating Summarise the logs", "Delegated Summarise the logs"],
      [TOOL_NAME.SPAWN, { task: "Watch the queue" }, "Starting agent Watch the queue", "Started agent Watch the queue"],
      [TOOL_NAME.SCHEDULE, { at: "2026-10-05T09:00:00Z", message: "Check the deploy" }, "Scheduling Check the deploy at 2026-10-05T09:00:00Z", "Scheduled Check the deploy at 2026-10-05T09:00:00Z"],
      [TOOL_NAME.CRON_ADD, { cron: "0 9 * * *", message: "Morning report" }, "Adding schedule 0 9 * * * Morning report", "Added schedule 0 9 * * * Morning report"],
      [TOOL_NAME.CRON_UPDATE, { schedule_id: "sch_1", cron: "0 10 * * *" }, "Updating schedule sch_1", "Updated schedule sch_1"],
      [TOOL_NAME.CRON_REMOVE, { job_id: 4 }, "Removing schedule 4", "Removed schedule 4"],
      [TOOL_NAME.CRON_RUN, { id: "sch_1" }, "Running schedule sch_1", "Ran schedule sch_1"],
      [TOOL_NAME.MESSAGE, { text: "On it" }, "Sending message On it", "Sent message On it"],
      [TOOL_NAME.MESSAGE, { content: "On it" }, "Sending message On it", "Sent message On it"],
      [TOOL_NAME.UPDATE_PLAN, { plan: [] }, "Updating plan", "Updated plan"],
      // Called without its main argument: nothing invented in its place.
      [TOOL_NAME.PUSHOVER, { priority: 1 }, "Notifying", "Notified"],
      [TOOL_NAME.BROWSER_OPEN, { wait: true }, "Opening", "Opened"],
      [TOOL_NAME.DELEGATE, { tools: [] }, "Delegating", "Delegated"],
      [TOOL_NAME.SPAWN, { tools: [] }, "Starting agent", "Started agent"],
      [TOOL_NAME.BROWSER, { action: "text", selector: "#main" }, "Reading #main", "Read #main"],
      [TOOL_NAME.BROWSER, { selector: "#x" }, "Browsing #x", "Browsed #x"],
      [TOOL_NAME.WRITE_STDIN, { chars: "q" }, 'Writing to terminal: "q"', 'Wrote to terminal: "q"'],
      [TOOL_NAME.SCHEDULE, { message: "Check" }, "Scheduling Check", "Scheduled Check"],
      [TOOL_NAME.CRON_ADD, { cron: "0 9 * * *" }, "Adding schedule 0 9 * * *", "Added schedule 0 9 * * *"],
    ];
    for (const [name, args, running, done] of cases) {
      expect([name, header(name, args, CELL_STATE.RUNNING)]).toEqual([name, running]);
      expect([name, header(name, args, CELL_STATE.SUCCEEDED)]).toEqual([name, done]);
    }
    // Argument names it does not know: the arguments as they came, never a guess.
    expect(header(TOOL_NAME.CRON_REMOVE, { name: "nightly" }, CELL_STATE.SUCCEEDED)).toBe('Removed schedule ({"name":"nightly"})');
    expect(header(TOOL_NAME.MESSAGE, { body: "x" }, CELL_STATE.SUCCEEDED)).toBe('Sent message ({"body":"x"})');
    // Arguments the runner dropped whole read as not recorded.
    expect(header(TOOL_NAME.GIT, undefined, CELL_STATE.SUCCEEDED)).toBe(`Ran ${ARGS_NOT_RECORDED}`);
    // A screenshot takes none, so none is no loss.
    expect(header(TOOL_NAME.SCREENSHOT, undefined, CELL_STATE.SUCCEEDED)).toBe("Took screenshot");
  });

  it("test_plan_copy_reads_its_steps", () => {
    const copy = toolCopy(TOOL_NAME.UPDATE_PLAN, {
      explanation: "Two left",
      plan: [
        { step: "Read the logs", status: "completed" },
        { step: "Find the cause", status: "in_progress" },
        { step: "Open a PR", status: "pending" },
        { step: "Bad status", status: "done" },
        "not a step",
      ],
    });
    expect(copy.body).toEqual({
      kind: TOOL_BODY.PLAN,
      explanation: "Two left",
      steps: [
        { step: "Read the logs", status: PLAN_STATUS.COMPLETED },
        { step: "Find the cause", status: PLAN_STATUS.IN_PROGRESS },
        { step: "Open a PR", status: PLAN_STATUS.PENDING },
      ],
    });
    expect(toolCopy(TOOL_NAME.UPDATE_PLAN, { plan: "x" }).body).toEqual({ kind: TOOL_BODY.PLAN, explanation: null, steps: [] });
    // A step or explanation at the leaf cap may be cut, and is marked so.
    const long = "s".repeat(ARGS_LEAF_MAX_BYTES);
    expect(toolCopy(TOOL_NAME.UPDATE_PLAN, { explanation: long, plan: [{ step: long, status: "pending" }] }).body).toEqual({
      kind: TOOL_BODY.PLAN, explanation: `${long}${CLIP_MARK}`, steps: [{ step: `${long}${CLIP_MARK}`, status: PLAN_STATUS.PENDING }],
    });
  });

  it("should mark each git word the runner may have cut, and no other", () => {
    const paths = Array.from({ length: 30 }, (_, at) => `src/file-${at}.ts`);
    // Thirty short words, over the cap only once joined: none was cut.
    expect(toolCopy(TOOL_NAME.GIT, { args: ["add", ...paths] }).target).toBe(["git", "add", ...paths].join(" "));
    const cut = "m".repeat(ARGS_LEAF_MAX_BYTES);
    expect(toolCopy(TOOL_NAME.GIT, { args: ["commit", "-m", cut, "--quiet"] }).target).toBe(`git commit -m ${cut}${CLIP_MARK} --quiet`);
    // Keys the runner may have cut carry the mark after their quotes.
    expect(header(TOOL_NAME.WRITE_STDIN, { chars: cut }, CELL_STATE.SUCCEEDED)).toBe(`Wrote to terminal: "${cut}"${CLIP_MARK}`);
  });

  it("test_patch_copy_reads_its_files_and_lines", () => {
    const { files, diff } = patchDiff(PATCH);
    expect(files).toEqual(["src/app.ts", "notes.md"]);
    expect(diff.rows).toEqual([
      { kind: DIFF_ROW.CONTEXT, text: "keep" },
      { kind: DIFF_ROW.REMOVED, text: "old" },
      { kind: DIFF_ROW.ADDED, text: "new" },
      { kind: DIFF_ROW.ADDED, text: "more" },
      { kind: DIFF_ROW.ADDED, text: "hello" },
    ]);
    expect([diff.added, diff.removed]).toEqual([3, 1]);
    expect(patchDiff("*** Update File: a\n*** Move to: b\nplain").files).toEqual(["a", "b"]);
    const short = "*** Update File: a.md\n-x\n+y";
    expect(toolCopy(TOOL_NAME.APPLY_PATCH, { patch: short })).toMatchObject({ target: "a.md", body: { kind: TOOL_BODY.PATCH } });
    const cut = `*** Update File: a.md\n+${"x".repeat(ARGS_LEAF_MAX_BYTES)}`;
    expect(toolCopy(TOOL_NAME.APPLY_PATCH, { patch: cut })).toEqual({
      verbs: { running: "Editing", done: "Edited" }, target: `a.md${CLIP_MARK}`, body: { kind: TOOL_BODY.CLIPPED_EDIT },
    });
    expect(toolCopy(TOOL_NAME.APPLY_PATCH, { patch: "x".repeat(ARGS_LEAF_MAX_BYTES) }).target).toBe("");
    // No patch text, or none in the patch grammar: the arguments as they came,
    // never "+0 −0" for a change nobody saw.
    expect(toolCopy(TOOL_NAME.APPLY_PATCH, { input: "*** Begin Patch" })).toEqual({
      verbs: { running: "Editing", done: "Edited" }, target: '({"input":"*** Begin Patch"})', body: { kind: TOOL_BODY.OUTPUT },
    });
    expect(toolCopy(TOOL_NAME.APPLY_PATCH, { patch: 7 }).target).toBe('({"patch":7})');
    expect(toolCopy(TOOL_NAME.APPLY_PATCH, { patch: "just words" })).toMatchObject({ target: '({"patch":"just words"})', body: { kind: TOOL_BODY.OUTPUT } });
  });
});
