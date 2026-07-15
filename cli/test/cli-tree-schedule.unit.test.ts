import { test, expect } from "bun:test";

import { VALID_ID, makeSpyTree, dispatch } from "./helpers-cli-tree.ts";

test("schedule add parses cron, timezone, message, and workspace", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "schedule",
    "add",
    VALID_ID,
    "--cron",
    "0 9 * * *",
    "--timezone",
    "Asia/Kolkata",
    "--message",
    "summarize",
    "--workspace",
    VALID_ID,
  ], handlers);
  expect(calls[0]?.name).toBe("schedule.add");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options.cron).toBe("0 9 * * *");
  expect(calls[0]?.frame.parsed.options.timezone).toBe("Asia/Kolkata");
  expect(calls[0]?.frame.parsed.options.message).toBe("summarize");
  expect(calls[0]?.frame.parsed.options.workspace).toBe(VALID_ID);
});

test("schedule list parses fleet id", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["schedule", "list", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("schedule.list");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe(VALID_ID);
});

test("schedule update parses schedule id and patch fields", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "schedule",
    "update",
    VALID_ID,
    VALID_ID,
    "--cron",
    "15 9 * * *",
    "--message",
    "again",
    "--status",
    "paused",
  ], handlers);
  expect(calls[0]?.name).toBe("schedule.update");
  expect(calls[0]?.frame.parsed.positionals).toEqual([VALID_ID, VALID_ID]);
  expect(calls[0]?.frame.parsed.options.status).toBe("paused");
});

test("schedule rm, status, and sync parse fleet and schedule ids", async () => {
  for (const verb of ["rm", "status", "sync"]) {
    const { handlers, calls } = makeSpyTree();
    await dispatch(["schedule", verb, VALID_ID, VALID_ID], handlers);
    expect(calls[0]?.name).toBe(`schedule.${verb}`);
    expect(calls[0]?.frame.parsed.positionals).toEqual([VALID_ID, VALID_ID]);
  }
});
