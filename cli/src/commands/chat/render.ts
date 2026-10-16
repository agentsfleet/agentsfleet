// Validated private state is written only to the invoking terminal.

import { Effect } from "effect";
import { OUTPUT_FORMAT, type Output } from "../../services/output.ts";
import { MESSAGE_ROLE, MESSAGE_STATE, SCHEDULE_SYNC, type Applied, type Message, type Review, type Fleet,
  type Schedule } from "./protocol.ts";

const JSON_INDENT = 2;
export const FLEET_ACTIVE = "active";
const INITIAL_RUN_PENDING = "pending";
const INITIAL_RUN_BLOCKED = "blocked";
const SCHEDULE_SYNC_FAILED = "schedule_sync_failed";
const SCHEDULE_SYNC_UNCONFIRMED = "schedule_sync_unconfirmed";

export function renderMessage(output: Output, message: Message): Effect.Effect<void> {
  if (output.format === OUTPUT_FORMAT.json) return output.info(JSON.stringify(message));
  const role = message.role === MESSAGE_ROLE.user ? "You" : "Assistant";
  if (message.failure) return output.warn(`${role}: ${message.state} (${message.failure.error_code}); ${message.failure.recovery_action}`);
  if (message.text) return output.info(`${role}: ${message.text}`);
  return message.state === MESSAGE_STATE.queued || message.state === MESSAGE_STATE.running
    ? output.info(`Message ${message.sequence}: ${message.state}`) : Effect.void;
}

export function renderReview(output: Output, review: Review): Effect.Effect<void> {
  if (output.format === OUTPUT_FORMAT.json) return output.info(JSON.stringify(review));
  return Effect.gen(function* () {
    yield* output.info(`Instance: ${review.effective.instance_name}`);
    yield* output.info(`Runtime model: ${review.effective.runtime_model_ref ?? "selection required"}`);
    yield* output.info(JSON.stringify(review.effective.definition, null, JSON_INDENT));
    yield* output.info(`Required access: ${JSON.stringify(review.requirements, null, JSON_INDENT)}`);
    for (const check of review.readiness) {
      yield* output.info(`${check.kind} ${check.subject}: ${check.state}${check.next_action?.href ? ` → ${check.next_action.href}` : ""}`);
    }
    yield* output.info(`Review digest: ${review.review_digest}`);
  });
}

export function renderFleet(output: Output, fleet: Fleet): Effect.Effect<void> {
  return Effect.gen(function* () {
    yield* renderResult(output, `Fleet ${fleet.name}: ${fleet.status}`, fleet);
    if (setupPartial(fleet)) {
      for (const check of fleet.setup.checks) {
        yield* output.warn(`${check.kind} ${check.subject}: ${check.state}${check.next_action?.href ? ` → ${check.next_action.href}` : ""}`);
      }
      if (initialPending(fleet)) yield* output.warn(`Initial run: ${String(fleet.start.state)}. Resume setup to recover the same admission.`);
    }
    yield* renderScheduleOwed(output, fleet.schedules);
  });
}

export function renderApplied(output: Output, applied: Applied): Effect.Effect<void> {
  return renderResult(output, `Applied setup revision ${applied.setup_revision}`, applied).pipe(
    Effect.andThen(renderScheduleOwed(output, applied.schedules)));
}

// A schedule saves before agentsfleet registers it with the external
// scheduler, so an active fleet can still have cron work that never wakes
// (docs/architecture/data_flow.md, "CLI partial success"). Every schedule not
// `synced` is owed a push: `failed` carries the scheduler's reason, and
// `syncing` after the response means the daemon's bounded wait gave up
// (rustd/crates/afd_api_tenant/src/handler/fleet/native/schedules.rs) and no
// background sweep finishes it. Each one names the `schedule sync` command
// (cli/src/program/tree/schedule.command.ts) that retries the push.
function renderScheduleOwed(output: Output, schedules: ReadonlyArray<Schedule>): Effect.Effect<void> {
  return Effect.forEach(schedules.filter(syncOwed), (schedule) => {
    const retry = `agentsfleet schedule sync ${schedule.fleet_id} ${schedule.schedule_id}`;
    const failed = schedule.sync === SCHEDULE_SYNC.failed;
    if (output.format === OUTPUT_FORMAT.json) {
      return output.printJsonErr({kind: failed ? SCHEDULE_SYNC_FAILED : SCHEDULE_SYNC_UNCONFIRMED,
        fleet_id: schedule.fleet_id, schedule_id: schedule.schedule_id, last_error: schedule.last_error ?? null, retry});
    }
    return output.warn(failed
      ? `Schedule ${schedule.schedule_id} is saved but not registered, so it will not fire${
        schedule.last_error ? ` (${schedule.last_error})` : ""}. Retry with: ${retry}`
      : `Schedule ${schedule.schedule_id} is saved but its registration was not confirmed, so it may not fire. Retry in a minute with: ${retry}`);
  }, {discard: true});
}

/** Chat emits one complete JSON value per line for streaming consumers. */
export function renderResult(output: Output, message: string, value: Record<string, unknown>): Effect.Effect<void> {
  return output.format === OUTPUT_FORMAT.json ? output.info(JSON.stringify(value)) : output.success(message, value);
}

export function isPartial(fleet: Fleet): boolean { return setupPartial(fleet) || schedulesOwed(fleet.schedules); }

export function schedulesOwed(schedules: ReadonlyArray<Schedule>): boolean { return schedules.some(syncOwed); }

// An unrecognised status counts as owed: only a confirmed `synced` lets the command exit zero.
function syncOwed(schedule: Schedule): boolean { return schedule.sync !== SCHEDULE_SYNC.synced; }

function setupPartial(fleet: Fleet): boolean { return fleet.status !== FLEET_ACTIVE || initialPending(fleet); }

function initialPending(fleet: Fleet): boolean {
  return fleet.start.state === INITIAL_RUN_PENDING || fleet.start.state === INITIAL_RUN_BLOCKED;
}
