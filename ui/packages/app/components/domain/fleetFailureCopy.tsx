"use client";

import type { MessageState } from "@assistant-ui/react";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import {
  readFailureDetail,
  readFailureLabel,
  readOutcome,
} from "./fleetMessageReaders";
import { failureSentenceFor, outcomeFor } from "@/lib/events/event-summary";

const STARTUP_FAILURE_TAG = "startup_posture";
const CHAT_STARTUP_FAILURE_LABEL =
  "This fleet needs instructions before it can respond.";
const RUNNER_REFUSAL_SENTENCE =
  "The runner refused this run before the fleet started.";

// Cause lines the runner reports when IT refuses a startup_posture lease before
// the fleet ever runs: the `DETAIL_*` constants the supervisor ends a lease
// with under `FailureClass::StartupPosture`, in
// rustd/crates/afr_supervisor/src/lease_loop.rs and its `lease_loop/` modules.
// They are the single source for telling a runner-side refusal apart from a
// fleet with no instructions. Matching is exact: any other detail keeps the
// needs-instructions sentence, and fleetFailureCopy.test.ts reads the Rust
// source so a reworded line fails a test instead of reaching a user.
export const RUNNER_REFUSAL_DETAILS = [
  "the worker pool was shutting down when the lease arrived",
  // Missing, the bundle refusal fell through to the needs-instructions
  // sentence and sent an operator to rewrite a fleet whose instructions were
  // fine: the runner had failed to fetch its bundle.
  "the fleet bundle could not be fetched and verified",
  "the fleet's memory could not be read",
  "the fleet names a tool this runner cannot host",
  "the fleet names a model provider this runner does not speak",
  "the fleet names a model endpoint at a private or reserved address",
  "the lease asked for a sandbox size outside the bounds a runner builds",
  "this host could not build a sandbox for the run",
  "the fleet bundle's support files could not be written to the workspace",
  "a bound repository could not be checked out into the workspace",
  "the egress allowlist could not be resolved into addresses this runner can admit",
  "the fleet allows an egress host at a private or reserved address",
] as const;

// Failure copy for the chat surface. Startup-posture failures get concise
// chat-specific wording; every other tag reuses the event-summary sentence.

export function messageOutcome(message: MessageState): string {
  const failureLabel = readFailureLabel(message);
  const failureDetail = readFailureDetail(message);
  const rawOutcome = readOutcome(message);
  if (!failureLabel) return rawOutcome;
  return formatFailureOutcome(failureLabel, rawOutcome, failureDetail);
}

export function eventOutcome(event: FleetEvent): string {
  if (!event.failureLabel) return event.outcome;
  return formatFailureOutcome(
    event.failureLabel,
    event.outcome,
    event.failureDetail,
  );
}

// The sentence depends on the cause, not only the tag: a startup_posture
// failure whose detail is one of the runner's refusal lines was refused by the
// runner, not starved of instructions.
function chatFailureSentenceFor(tag: string, cause: string | null): string {
  if (tag !== STARTUP_FAILURE_TAG) return failureSentenceFor(tag);
  if (cause === null) return CHAT_STARTUP_FAILURE_LABEL;
  if (isRunnerRefusal(cause)) return RUNNER_REFUSAL_SENTENCE;
  // An internal identifier names no cause a user can act on, so it cannot
  // support the claim that their fleet lacks instructions. The one thing it does
  // establish is that the runner failed before the fleet ran — report that.
  // Without this arm a runner-side fault (a daemon that cannot load its config,
  // say) is shown to the user as their own misconfiguration.
  if (isInternalIdentifier(cause)) return RUNNER_REFUSAL_SENTENCE;
  return CHAT_STARTUP_FAILURE_LABEL;
}

function isRunnerRefusal(cause: string): boolean {
  return (RUNNER_REFUSAL_DETAILS as readonly string[]).includes(cause);
}

// Cause lines written for operators are prose and always contain whitespace
// ("this host could not build a sandbox for the run"). A single bare token —
// `FleetInitFailed` — is an internal error name that leaked through a layer
// which should have mapped it, and it is never shown to the user.
function isInternalIdentifier(cause: string): boolean {
  return !/\s/.test(cause);
}

function formatFailureOutcome(
  tag: string,
  rawOutcome: string,
  detail: string | null,
): string {
  const embeddedDetail = rawOutcome.split("—").slice(1).join("—").trim();
  const cause = detail ?? (embeddedDetail.length > 0 ? embeddedDetail : null);
  if (tag === "runner_crash") {
    return outcomeFor({ status: "fleet_error", failure_label: tag, failure_detail: cause });
  }
  const sentence = chatFailureSentenceFor(tag, cause);
  // The sentence is classified from the raw cause, but only prose is shown: an
  // internal identifier appended after an em-dash reads as diagnostic detail
  // while telling the user nothing.
  const shown = cause !== null && !isInternalIdentifier(cause) ? cause : null;
  return shown ? `${sentence} — ${shown}` : sentence;
}
