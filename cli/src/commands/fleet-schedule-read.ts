import { Effect, Schema } from "effect";
import { ServerError } from "../errors/index.ts";
import { isInstant } from "../lib/instant.ts";
import { AGE_KEY } from "../output/index.ts";

const text = Schema.String.check(Schema.makeFilter(value => value.length > 0));
const instant = Schema.String.check(Schema.makeFilter(isInstant));
// Every consumed field is sent by afd_api_wire::schedule::View, including nullable next_run_at.
const ScheduleSchema = Schema.Struct({
  schedule_id: text, fleet_id: text, cron: text, timezone: text, message: text,
  status: text, sync: text, description: text, run_state: text,
  next_run_at: Schema.NullOr(instant), created_at: instant, updated_at: instant,
});
const SchedulePageSchema = Schema.Struct({schedules: Schema.Array(ScheduleSchema)});
type Schedule = typeof ScheduleSchema.Type;
const RUN_STATE = {scheduled: "scheduled", paused: "paused", pending: "activation_pending", unavailable: "next_run_unavailable"} as const;
const UNKNOWN_TIME = "—";
const RUN_UNAVAILABLE = "Next run unavailable";
const BAD_GATEWAY = 502;
export const SCHEDULE_RESPONSE_INVALID = "SCHEDULE_RESPONSE_INVALID";

function decode<T>(schema: Schema.Codec<T, unknown>, value: unknown): Effect.Effect<T, ServerError> {
  return Schema.decodeUnknownEffect(schema)(value).pipe(Effect.mapError(() => new ServerError({
    detail: "The schedule response did not match the supported API shape",
    suggestion: "update the CLI and verify that it targets the matching daemon build",
    code: SCHEDULE_RESPONSE_INVALID, status: BAD_GATEWAY, requestId: null,
  })));
}

export const decodeSchedule = (value: unknown) => decode(ScheduleSchema, value);
export const decodeSchedules = (value: unknown) => decode(SchedulePageSchema, value);

function scheduleState(row: Schedule): string {
  switch (row.run_state) {
    case RUN_STATE.scheduled: return row.next_run_at === null ? RUN_UNAVAILABLE : "Scheduled";
    case RUN_STATE.paused: return "Paused";
    case RUN_STATE.pending: return "Activation pending";
    case RUN_STATE.unavailable: return RUN_UNAVAILABLE;
    default: return "Status unavailable";
  }
}

function nextRun(row: Schedule): string | null {
  return row.run_state === RUN_STATE.scheduled ? row.next_run_at : null;
}

export function scheduleSummary(row: Schedule): string {
  const next = nextRun(row);
  return `${row.description} · ${scheduleState(row)}${next === null ? "" : ` · Next run: ${next}`}`;
}

export function scheduleRows(rows: readonly Schedule[]): ReadonlyArray<Record<string, unknown>> {
  return rows.map(row => ({schedule_id: row.schedule_id, description: row.description,
    status: scheduleState(row), next_run_at: nextRun(row) ?? UNKNOWN_TIME,
    message: row.message, [AGE_KEY]: row.created_at}));
}
