import type { MessageState } from "@assistant-ui/react";
import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { describe, expect, it } from "vitest";
import { CELL_STATE } from "./tool-call-copy";
import { toolActivity, toolGroupActivity } from "./tool-call-activity";
import { ARGS_NOT_RECORDED, TOOL_NAME } from "./tool-call-shape";

const GITHUB = "https://api.github.com/repos/agentsfleet/linkwarden";
const SUCCESS = CELL_STATE.SUCCEEDED;
const RUNNING = CELL_STATE.RUNNING;

describe("recorded tool activity", () => {
  it.each([
    [TOOL_NAME.FILE_READ, {path: "/workspace/a.ts"}, "Reading file", "Read file", "a.ts"],
    [TOOL_NAME.FILE_READ_HASHED, {path: "a.ts"}, "Reading file", "Read file", "a.ts"],
    [TOOL_NAME.FILE_WRITE, {path: "a.ts"}, "Writing", "Wrote", "a.ts"],
    [TOOL_NAME.FILE_APPEND, {path: "a.ts"}, "Appending", "Appended", "a.ts"],
    [TOOL_NAME.FILE_EDIT, {path: "a.ts"}, "Editing", "Edited", "a.ts"],
    [TOOL_NAME.FILE_EDIT_HASHED, {path: "a.ts"}, "Editing", "Edited", "a.ts"],
    [TOOL_NAME.FILE_DELETE, {path: "a.ts"}, "Deleting", "Deleted", "a.ts"],
    [TOOL_NAME.APPLY_PATCH, {}, "Editing files", "Edited files", ""],
    [TOOL_NAME.EXEC_COMMAND, {cmd: "bun run test\necho later"}, "Running command", "Ran command", "bun run test"],
    [TOOL_NAME.SHELL, {command: "ls"}, "Running command", "Ran command", "ls"],
    [TOOL_NAME.MEMORY_STORE, {key: "finding"}, "Saving memory", "Saved memory", "finding"],
    [TOOL_NAME.MEMORY_FORGET, {key: "finding"}, "Forgetting memory", "Forgot memory", "finding"],
    [TOOL_NAME.MEMORY_RECALL, {query: "finding"}, "Recalling memory", "Recalled memory", "finding"],
    [TOOL_NAME.MEMORY_LIST, {}, "Listing", "Listed", "memories"],
    [TOOL_NAME.CRON_LIST, {}, "Listing", "Listed", "schedules"],
    [TOOL_NAME.CRON_RUNS, {}, "Listing", "Listed", "schedule runs"],
    [TOOL_NAME.GIT, {args: ["rev-parse", "--show-toplevel"]}, "Scoping workspace", "Scoped workspace", ""],
    [TOOL_NAME.GIT, {args: ["status", "--short"]}, "Checking changes", "Checked changes", ""],
    [TOOL_NAME.GIT, {args: ["diff"]}, "Checking changes", "Checked changes", ""],
    [TOOL_NAME.GIT, {args: ["log"]}, "Running Git command", "Ran Git command", ""],
    ["unknown_tool", {credential: "hidden-input"}, "Using", "Used", "unknown_tool"],
    [TOOL_NAME.UPDATE_PLAN, {plan: []}, "Updating plan", "Updated plan", TOOL_NAME.UPDATE_PLAN],
    [TOOL_NAME.HTTP_REQUEST, {url: `${GITHUB}/pulls`, method: "POST"}, "Opening pull request", "Opened pull request", "agentsfleet/linkwarden"],
    [TOOL_NAME.HTTP_REQUEST, {url: `${GITHUB}/git/trees/sha?recursive=1`}, "Listing files", "Listed files", "agentsfleet/linkwarden"],
    [TOOL_NAME.HTTP_REQUEST, {url: `${GITHUB}/contents`}, "Listing files", "Listed files", "agentsfleet/linkwarden"],
    [TOOL_NAME.HTTP_REQUEST, {url: `${GITHUB}/contents/apps/web/a.ts`}, "Reading file", "Read file", "apps/web/a.ts"],
    [TOOL_NAME.HTTP_REQUEST, {url: `${GITHUB}/actions/jobs/77/logs`}, "Reading job log", "Read job log", "agentsfleet/linkwarden"],
    [TOOL_NAME.HTTP_REQUEST, {url: GITHUB}, "Checking repository access", "Checked repository access", "agentsfleet/linkwarden"],
    [TOOL_NAME.HTTP_REQUEST, {url: "https://api.github.com/user"}, "Making request", "Made request", "GET https://api.github.com/user"],
    [TOOL_NAME.HTTP_REQUEST, {url: `${GITHUB}/issues`}, "Making request", "Made request", `GET ${GITHUB}/issues`],
    [TOOL_NAME.HTTP_REQUEST, {url: "https://api.example.test/readyz"}, "Checking environment", "Checked environment", "https://api.example.test"],
    [TOOL_NAME.HTTP_REQUEST, {url: "https://api.example.test/healthz"}, "Checking environment", "Checked environment", "https://api.example.test"],
    [TOOL_NAME.HTTP_REQUEST, {url: "https://api.example.test/v1/workspaces/owner"}, "Checking workspace access", "Checked workspace access", "owner"],
    [TOOL_NAME.HTTP_REQUEST, {url: "https://api.example.test/readyz", method: "POST"}, "Making request", "Made request", "POST https://api.example.test/readyz"],
    [TOOL_NAME.HTTP_REQUEST, {url: "https://user:password@example.test/path?token=hidden#fragment"}, "Making request", "Made request", "GET https://example.test/path"],
    [TOOL_NAME.HTTP_REQUEST, {url: "not a URL"}, "Making request", "Made request", ""],
  ] satisfies [string, ToolArgs, string, string, string][])("names the actual action %#", (name, args, running, done, target) => {
    expect(toolActivity(name, args, RUNNING)).toMatchObject({label: running, target, state: RUNNING});
    expect(toolActivity(name, args, SUCCESS)).toMatchObject({label: done, target, state: SUCCESS});
  });

  it("distinguishes failed, interrupted and unconfirmed requests from completed effects", () => {
    for (const state of [CELL_STATE.FAILED, CELL_STATE.INTERRUPTED, CELL_STATE.SETTLED]) {
      const activity = toolActivity(TOOL_NAME.HTTP_REQUEST, {url: `${GITHUB}/pulls`, method: "POST"}, state);
      expect(activity.label).not.toContain("Opened"); expect(activity.state).toBe(state);
      expect(activity.label).toMatch(/^(Failed|Interrupted|Outcome unavailable): opening pull request$/);
    }
    expect(toolActivity(TOOL_NAME.FILE_READ, undefined, RUNNING).target).toBe(ARGS_NOT_RECORDED);
    expect(toolActivity(TOOL_NAME.HTTP_REQUEST, undefined, RUNNING).target).toBe(ARGS_NOT_RECORDED);
    expect(toolActivity(TOOL_NAME.EXEC_COMMAND, undefined, RUNNING).target).toBe("");
    expect(toolActivity(TOOL_NAME.GIT, undefined, RUNNING).label).toBe("Running Git command");
  });

  it("does not inspect full file contents or diffs to label a collapsed action", () => {
    const args = Object.defineProperties({path: "large.ts"}, {
      content: {get() {throw new Error("large content was read");}}, old_text: {get() {throw new Error("diff was parsed");}},
    });
    expect(toolActivity(TOOL_NAME.FILE_WRITE, args, RUNNING).target).toBe("large.ts");
    expect(toolActivity(TOOL_NAME.FILE_EDIT, args, SUCCESS).label).toBe("Edited");
  });

  it("summarizes only the group's own actions and preserves mixed outcomes", () => {
    const content = [part("file_read", SUCCESS), {type: "text", text: "reply"}, part("file_write", CELL_STATE.FAILED), part("unknown", SUCCESS)] satisfies MessageState["content"];
    expect(toolGroupActivity(content, [0], false).label).toBe("Read sources");
    expect(toolGroupActivity(content, [0, 2, 3], false)).toMatchObject({state: CELL_STATE.FAILED, label: "Read sources, Failed, Used tools"});
    expect(toolGroupActivity([part("file_read", SUCCESS), part("file_read", SUCCESS)], [0, 1], false).label).toBe("Read sources (2)");
    expect(toolGroupActivity([part("file_read", CELL_STATE.INTERRUPTED), part("file_read", SUCCESS)], [0, 1], false).state).toBe(CELL_STATE.INTERRUPTED);
    expect(toolGroupActivity([part("unknown")], [0], false).label).toBe("Interrupted");
    expect(toolGroupActivity([{...part("unknown"), result: {}}], [0], false).label).toBe("Outcome unavailable");
    expect(toolGroupActivity(content, [1, 99], false)).toMatchObject({label: "Tool activity", state: CELL_STATE.SETTLED});
  });

  it("shows current work and the number of concurrently active calls", () => {
    expect(toolGroupActivity([part("file_read")], [0], true)).toMatchObject({label: "Reading file", target: "a.ts", state: RUNNING});
    const group = toolGroupActivity([part("file_read"), part("file_write")], [0, 1], true);
    expect(group.state).toBe(RUNNING); expect(group.target).toBe("a.ts · 2 active");
  });
});

function part(toolName: string, state?: Exclude<(typeof CELL_STATE)[keyof typeof CELL_STATE], "running" | "settled">): Extract<MessageState["content"][number], {type: "tool-call"}> {
  return {type: "tool-call", toolCallId: toolName, toolName, args: {path: "a.ts"}, argsText: '{"path":"a.ts"}',
    ...(state === undefined ? {} : {result: {status: state}})};
}
