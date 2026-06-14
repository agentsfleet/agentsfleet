// Read-only `agentsfleet billing show [--limit N] [--cursor TOKEN] [--json]`.
//
// Calls GET /v1/tenants/me/billing for the balance card and GET
// /v1/tenants/me/billing/charges for the per-event drain history. Each event
// produces up to two charge rows (charge_type ∈ {receive, stage}); the CLI
// groups them by event_id so each row in the table represents one event with
// both charges combined. `--json` emits the raw shape for scripting and
// includes `next_cursor` so callers can paginate.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import { CHARGE_TYPE, formatDollars } from "../constants/billing.ts";
import { TENANT_BILLING_PATH } from "../lib/api-paths.ts";
import { ValidationError, type CliError } from "../errors/index.ts";

const CHARGES_PATH = `${TENANT_BILLING_PATH}/charges`;
const BILLING_DASHBOARD_URL = "https://app.usezombie.com/settings/billing";
const PURCHASE_FOOTER_LINE_2 =
  "Stripe purchase ships in v2.1; for now contact support for a top-up.";

const DEFAULT_LIMIT = 10;
const MAX_LIMIT = 100;

interface BillingSnapshot {
  readonly balance_nanos?: number;
  readonly is_exhausted?: boolean;
}

interface ChargeRow {
  readonly event_id?: string;
  readonly posture?: string;
  readonly model?: string;
  readonly recorded_at?: number | null;
  readonly charge_type?: string;
  readonly credit_deducted_nanos?: number;
  readonly token_count_input?: number | null;
  readonly token_count_output?: number | null;
}

interface ChargesResponse {
  readonly items?: ReadonlyArray<ChargeRow>;
  readonly next_cursor?: string | null;
}

interface EventSummary {
  event_id: string | undefined;
  posture: string | undefined;
  model: string | undefined;
  recorded_at: number | null;
  receive_nanos: number;
  stage_nanos: number;
  token_count_input: number | null;
  token_count_output: number | null;
  total_nanos: number;
}

export interface BillingShowArgs {
  readonly limit: number | string | undefined;
  readonly cursor: string | undefined;
}

const parseLimit = (
  raw: number | string | undefined,
): Effect.Effect<number, ValidationError> => {
  if (raw === undefined || raw === null) return Effect.succeed(DEFAULT_LIMIT);
  const n = typeof raw === "number" ? raw : Number.parseInt(String(raw), 10);
  if (!Number.isFinite(n) || n <= 0 || n > MAX_LIMIT) {
    return Effect.fail(
      new ValidationError({
        detail: `--limit must be an integer between 1 and ${MAX_LIMIT}`,
        suggestion: `pass --limit <1..${MAX_LIMIT}>`,
      }),
    );
  }
  return Effect.succeed(n);
};

const parseCursor = (
  raw: string | undefined,
): Effect.Effect<string | null, ValidationError> => {
  if (raw === undefined || raw === null) return Effect.succeed(null);
  if (raw.length === 0) {
    return Effect.fail(
      new ValidationError({
        detail: "--cursor must not be empty",
        suggestion: "pass --cursor <next_cursor> from a previous page",
      }),
    );
  }
  return Effect.succeed(raw);
};

const groupRowsByEvent = (
  rows: ReadonlyArray<ChargeRow>,
): EventSummary[] => {
  const byEvent = new Map<string | undefined, EventSummary>();
  for (const r of rows) {
    let entry = byEvent.get(r.event_id);
    if (!entry) {
      entry = {
        event_id: r.event_id,
        posture: r.posture,
        model: r.model,
        recorded_at: r.recorded_at ?? null,
        receive_nanos: 0,
        stage_nanos: 0,
        token_count_input: null,
        token_count_output: null,
        total_nanos: 0,
      };
      byEvent.set(r.event_id, entry);
    }
    if (r.charge_type === CHARGE_TYPE.receive) {
      entry.receive_nanos = r.credit_deducted_nanos ?? 0;
    } else if (r.charge_type === CHARGE_TYPE.stage) {
      entry.stage_nanos = r.credit_deducted_nanos ?? 0;
      entry.token_count_input = r.token_count_input ?? null;
      entry.token_count_output = r.token_count_output ?? null;
    }
    // The receive row is recorded first; pin recorded_at to the earliest of
    // the two so sort-by-time matches the event's gate-pass moment.
    if (
      r.recorded_at != null &&
      (entry.recorded_at == null || r.recorded_at < entry.recorded_at)
    ) {
      entry.recorded_at = r.recorded_at;
    }
    entry.total_nanos = entry.receive_nanos + entry.stage_nanos;
  }
  return Array.from(byEvent.values()).sort(
    (a, b) => (b.recorded_at ?? 0) - (a.recorded_at ?? 0),
  );
};

const renderHuman = (
  balance: BillingSnapshot | null,
  events: ReadonlyArray<EventSummary>,
  nextCursor: string | null,
): Effect.Effect<void, never, Output> =>
  Effect.gen(function* () {
    const output = yield* Output;
    const balanceNanos = balance?.balance_nanos ?? 0;
    yield* output.info(`Tenant balance:    ${formatDollars(balanceNanos)}`);
    yield* output.info("");

    if (events.length === 0) {
      yield* output.info("No billable events recorded yet.");
    } else {
      yield* output.info(`Last ${events.length} events drained credits:`);
      yield* output.printTable(
        [
          { key: "event_id", label: "EVENT_ID" },
          { key: "posture", label: "POSTURE" },
          { key: "model", label: "MODEL" },
          { key: "in_tok", label: "IN_TOK" },
          { key: "out_tok", label: "OUT_TOK" },
          { key: "receive", label: "RECEIVE" },
          { key: "stage", label: "STAGE" },
          { key: "total", label: "TOTAL" },
        ],
        events.map((e) => ({
          event_id: e.event_id ?? "",
          posture: e.posture ?? "",
          model: e.model ?? "",
          in_tok: e.token_count_input != null ? String(e.token_count_input) : LITERAL,
          out_tok: e.token_count_output != null ? String(e.token_count_output) : LITERAL,
          receive: formatDollars(e.receive_nanos),
          stage: formatDollars(e.stage_nanos),
          total: formatDollars(e.total_nanos),
        })),
      );
    }

    if (nextCursor) {
      yield* output.info("");
      yield* output.info(
        `(more events available — re-run with --cursor ${nextCursor})`,
      );
    }
    yield* output.info("");
    if (balance?.is_exhausted) {
      yield* output.error(`⚠ Out of credits. See ${BILLING_DASHBOARD_URL}`);
    } else {
      yield* output.info(`ⓘ Out of credits? See ${BILLING_DASHBOARD_URL}`);
    }
    yield* output.info(`   ${PURCHASE_FOOTER_LINE_2}`);
  });

export const billingShowEffectFromArgs = (
  args: BillingShowArgs,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;

    const limit = yield* parseLimit(args.limit);
    const cursor = yield* parseCursor(args.cursor);

    // Charges limit is `limit * 2` because each event has up to 2 rows
    // (receive + stage); after grouping we slice to the user-requested
    // event count. `--cursor` is forwarded verbatim — the server treats
    // it as opaque.
    const chargesPath = cursor
      ? `${CHARGES_PATH}?limit=${limit * 2}&cursor=${encodeURIComponent(cursor)}`
      : `${CHARGES_PATH}?limit=${limit * 2}`;
    const [billing, charges] = yield* Effect.all(
      [
        http.request<BillingSnapshot>({ path: TENANT_BILLING_PATH, token }),
        http.request<ChargesResponse>({ path: chargesPath, token }),
      ],
      { concurrency: 2 },
    );

    const events = groupRowsByEvent(charges?.items ?? []).slice(0, limit);
    const nextCursor = charges?.next_cursor ?? null;

    if (config.jsonMode) {
      yield* output.printJson({
        balance_nanos: billing?.balance_nanos ?? 0,
        is_exhausted: Boolean(billing?.is_exhausted),
        events,
        next_cursor: nextCursor,
      });
      return;
    }
    yield* renderHuman(billing, events, nextCursor);
  });
const LITERAL = "—" as const;
