import { expect, it } from "vitest";
import { chatRecovery, isStopped, UNKNOWN_USAGE_REASON } from "@/lib/api/draft-recovery";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import type { DraftView } from "@/lib/api/draft-state";
import { DRAFT, MESSAGE } from "./helpers/draft-fixtures";

const UNKNOWN = {...MESSAGE, state: "failed" as const,
  failure: {reason: UNKNOWN_USAGE_REASON, error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"}};
const VIEW: DraftView = {...EMPTY_DRAFT_VIEW, draft: DRAFT, messages: [UNKNOWN]};

it("draft_recovery_uses_current_watermark even when cached actions still say await accounting", () => {
  expect(chatRecovery(VIEW)).toEqual({pending: false, blocked: true, canStop: true});
  expect(chatRecovery({...VIEW, draft: {...DRAFT, cancelled_through: MESSAGE.sequence}}))
    .toEqual({pending: false, blocked: false, canStop: false});
  expect(chatRecovery({...VIEW, draft: {...DRAFT, cancelled_through: MESSAGE.sequence},
    messages: [UNKNOWN, {...UNKNOWN, sequence: "2"}]}))
    .toEqual({pending: false, blocked: true, canStop: true});
});

it.each(["queued", "running"] as const)("keeps %s work stoppable until its watermark", (state) => {
  const view = {...VIEW, messages: [{...MESSAGE, state}]};
  expect(chatRecovery(view)).toEqual({pending: true, blocked: false, canStop: true});
  expect(chatRecovery({...view, draft: {...DRAFT, cancelled_through: MESSAGE.sequence}}))
    .toEqual({pending: false, blocked: false, canStop: false});
});

it.each(["completed", "cancelled", "failed", "timed_out"] as const)("does not treat an ordinary %s row as live work", (state) => {
  expect(chatRecovery({...VIEW, messages: [{...MESSAGE, state}]}))
    .toEqual({pending: false, blocked: false, canStop: false});
});

it("requires an accessible editing draft for Stop", () => {
  for (const view of [{...VIEW, draft: null}, {...VIEW, inaccessible: true},
    {...VIEW, draft: {...DRAFT, state: "closed" as const}}]) {
    expect(chatRecovery(view)).toEqual({pending: false, blocked: true, canStop: false});
  }
  expect(chatRecovery(EMPTY_DRAFT_VIEW)).toEqual({pending: false, blocked: false, canStop: false});
});

it("compares full decimal sequences above JavaScript's safe integer range", () => {
  expect(isStopped({...MESSAGE, sequence: "9007199254740993"}, "9007199254740992")).toBe(false);
  expect(isStopped({...MESSAGE, sequence: "9007199254740993"}, "9007199254740993")).toBe(true);
});
