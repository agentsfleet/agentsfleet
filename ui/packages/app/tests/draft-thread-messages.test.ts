import { expect, it } from "vitest";
import { draftThreadMessages } from "@/lib/api/draft-thread-messages";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { UNKNOWN_USAGE_REASON } from "@/lib/api/draft-recovery";
import { DRAFT, MESSAGE } from "./helpers/draft-fixtures";

const causes = [
  {cause: "deadline", text: "The model did not finish before its request deadline."},
  {cause: "cancelled", text: "The model request was stopped."},
  {cause: "provider_interrupted", text: "The connection to the model ended before completion was confirmed."},
  {cause: "completion_unverified", text: "The model reply did not confirm a completed answer."},
] as const;

it.each(causes)("shows the recorded $cause without suggesting another model request", ({cause, text}) => {
  for (const reason of ["provider_failed", UNKNOWN_USAGE_REASON]) {
    const failure = {reason, cause, error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"};
    const rows = draftThreadMessages({...EMPTY_DRAFT_VIEW, draft: DRAFT,
      messages: [{...MESSAGE, role: "assistant", state: "failed", text: undefined, failure}]});
    const suffix = reason === UNKNOWN_USAGE_REASON ? " Usage is still being reconciled." : "";
    expect(rows).toMatchObject([{content: [{type: "text", text: `UZ-AUTHOR-015: ${text}${suffix}`}], status: {type: "complete"}}]);
  }
});

it("retains the recorded cause after Stop while acknowledging unsettled earlier usage", () => {
  const rows = draftThreadMessages({...EMPTY_DRAFT_VIEW, draft: {...DRAFT, cancelled_through: MESSAGE.sequence},
    messages: [{...MESSAGE, role: "assistant", state: "failed", text: undefined,
      failure: {reason: UNKNOWN_USAGE_REASON, cause: "cancelled", error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"}}]});
  expect(rows).toMatchObject([{content: [{type: "text",
    text: "UZ-AUTHOR-015: The model request was stopped. This turn is stopped; earlier usage is still being reconciled."}]}]);
});

it("uses the safe unknown-usage explanation for a stopped older turn without a recorded cause", () => {
  const rows = draftThreadMessages({...EMPTY_DRAFT_VIEW, draft: {...DRAFT, cancelled_through: MESSAGE.sequence},
    messages: [{...MESSAGE, role: "assistant", state: "failed", text: undefined,
      failure: {reason: UNKNOWN_USAGE_REASON, error_code: "UZ-AUTHOR-015", recovery_action: "await_accounting"}}]});
  expect(rows).toMatchObject([{content: [{type: "text",
    text: "UZ-AUTHOR-015: The model request ended without confirmed usage. This turn is stopped; earlier usage is still being reconciled."}]}]);
});

it("shows matching live text while running and preserves the durable answer after completion", () => {
  const liveReply = {sequence: MESSAGE.sequence, stream: "private-stream", text: "Partial answer", gapped: false};
  const view = {...EMPTY_DRAFT_VIEW, draft: DRAFT, liveReply};
  expect(draftThreadMessages({...view, messages: [{...MESSAGE, state: "running"}]})).toMatchObject([
    {role: "user", content: [{text: MESSAGE.text}]},
    {role: "assistant", content: [{text: liveReply.text}], status: {type: "running"}},
  ]);
  expect(draftThreadMessages({...view, messages: [{...MESSAGE, role: "assistant", text: "Durable answer"}]}))
    .toMatchObject([{content: [{text: "Durable answer"}], status: {type: "complete"}}]);
});
