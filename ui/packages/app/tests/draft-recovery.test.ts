import { expect, it } from "vitest";
import { chatRecovery, UNKNOWN_USAGE_REASON, unfinishedTurns } from "@/lib/api/draft-recovery";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import type { DraftView } from "@/lib/api/draft-state";
import { DRAFT, MESSAGE } from "./helpers/draft-fixtures";

const UNKNOWN = {...MESSAGE, state: "failed" as const,
  failure: {reason: UNKNOWN_USAGE_REASON, error_code: "UZ-AUTHOR-015", recovery_action: "retry_message"}};
const VIEW: DraftView = {...EMPTY_DRAFT_VIEW, draft: DRAFT, messages: [UNKNOWN]};

it("an unknown-usage failure neither holds the chat nor offers Stop", () => {
  expect(chatRecovery(VIEW)).toEqual({pending: false, canStop: false});
  expect(chatRecovery({...VIEW, messages: [UNKNOWN, {...MESSAGE, sequence: "2", state: "queued" as const}]}))
    .toEqual({pending: true, canStop: true});
});

it.each(["queued", "running"] as const)("keeps %s work stoppable", (state) => {
  expect(chatRecovery({...VIEW, messages: [{...MESSAGE, state}]})).toEqual({pending: true, canStop: true});
});

it.each(["completed", "cancelled", "failed", "timed_out"] as const)("does not treat an ordinary %s row as live work", (state) => {
  expect(chatRecovery({...VIEW, messages: [{...MESSAGE, state}]}))
    .toEqual({pending: false, canStop: false});
});

it("requires an accessible editing draft for Stop", () => {
  const running = {...VIEW, messages: [{...MESSAGE, state: "running" as const}]};
  for (const view of [{...running, draft: null}, {...running, inaccessible: true},
    {...running, draft: {...DRAFT, state: "closed" as const}}]) {
    expect(chatRecovery(view)).toEqual({pending: true, canStop: false});
  }
  expect(chatRecovery(EMPTY_DRAFT_VIEW)).toEqual({pending: false, canStop: false});
});

it("names each unfinished request's turn once, oldest first, for Stop", () => {
  const second = "0199a801-1000-7000-8000-0000000000f2";
  const third = "0199a801-1000-7000-8000-0000000000f3";
  expect(unfinishedTurns([MESSAGE, {...MESSAGE, role: "assistant", state: "running", turn_id: second},
    {...MESSAGE, state: "running", turn_id: second}, {...MESSAGE, state: "queued", turn_id: third}])).toEqual([second, third]);
});
