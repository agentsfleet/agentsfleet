// Read-only `agentsfleet billing show [--limit N] [--cursor TOKEN] [--json]`.
//
// Calls GET /v1/tenants/me/billing for the balance card and GET
// /v1/tenants/me/billing/charges for runtime and settled setup costs. Runtime
// receive/stage rows on the same page group by (fleet_id,event_id); setup rows
// retain their independent attempt identity. Keep every returned cost before
// forwarding the server cursor, or pagination would silently skip money.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import { CHARGE_TYPE, formatDollars } from "../constants/billing.ts";
import { TENANT_BILLING_PATH } from "../lib/api-paths.ts";
import { ValidationError, type CliError } from "../errors/index.ts";
import { EMPTY_CELL } from "../output/index.ts";
import { byInstant, instantMs } from "../lib/instant.ts";
import { decodeBilling, decodeCharges, type BillingSnapshot, type ChargeRow } from "./billing-schema.ts";

const CHARGES_PATH = `${TENANT_BILLING_PATH}/charges`;
const BILLING_DASHBOARD_URL = "https://app.agentsfleet.net/settings/billing";
const PURCHASE_FOOTER_LINE_2 =
  "Stripe purchase ships in v2.1; for now contact support for a top-up.";

const BILLING_SHOWN = "Billing" as const;
const DEFAULT_LIMIT = 10;
const MAX_LIMIT = 100;
const LIMIT_LITERAL = /^\d+$/;

interface EventSummary {
  fleet_id: string | null | undefined;
  event_id: string | null | undefined;
  authoring_attempt_id?: string;
  setup_nanos?: string;
  posture: string | undefined;
  model: string | undefined;
  recorded_at: string | null;
  receive_nanos: string;
  stage_nanos: string;
  token_count_input: number | null;
  token_count_output: number | null;
  total_nanos: string;
}

export interface BillingShowArgs {
  readonly limit: number | string | undefined;
  readonly cursor: string | undefined;
}

const parseLimit = (
  raw: number | string | undefined,
): Effect.Effect<number, ValidationError> => {
  if (raw === undefined || raw === null) return Effect.succeed(DEFAULT_LIMIT);
  const n = typeof raw === "number" || LIMIT_LITERAL.test(raw) ? Number(raw) : Number.NaN;
  if (!Number.isInteger(n) || n <= 0 || n > MAX_LIMIT) {
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

// A charge belongs to one fleet's event, so the key is both. NUL separates
// them because daemon identifiers cannot contain one, unlike an ambiguous
// plain concatenation.
const groupKey = (row: ChargeRow): string =>
  row.authoring_attempt_id != null
    ? `${CHARGE_TYPE.authoring}\u0000${row.authoring_attempt_id}`
    : `${row.fleet_id ?? ""}\u0000${row.event_id ?? ""}`;

const addNanos = (a: string, b: string): string => String(BigInt(a) + BigInt(b));
const addCount = (a: number | null, b: number | null | undefined): number | null =>
  a === null && b == null ? null : (a ?? 0) + (b ?? 0);

const applyCharge = (entry: EventSummary, row: ChargeRow): void => {
  if (row.charge_type === CHARGE_TYPE.receive) {
    entry.receive_nanos = addNanos(entry.receive_nanos, row.credit_deducted_nanos);
  } else if (row.charge_type === CHARGE_TYPE.stage || row.charge_type === CHARGE_TYPE.authoring) {
    if (row.charge_type === CHARGE_TYPE.authoring) {
      if (row.authoring_attempt_id != null) entry.authoring_attempt_id = row.authoring_attempt_id;
      entry.setup_nanos = addNanos(entry.setup_nanos ?? "0", row.credit_deducted_nanos);
    } else {
      entry.stage_nanos = addNanos(entry.stage_nanos, row.credit_deducted_nanos);
    }
    entry.token_count_input = addCount(entry.token_count_input, row.token_count_input);
    entry.token_count_output = addCount(entry.token_count_output, row.token_count_output);
  }
  // Every returned debit contributes, including a future category this client cannot label.
  entry.total_nanos = addNanos(entry.total_nanos, row.credit_deducted_nanos);
};

const groupRowsByEvent = (
  rows: ReadonlyArray<ChargeRow>,
): EventSummary[] => {
  const byEvent = new Map<string, EventSummary>();
  for (const r of rows) {
    const key = groupKey(r);
    let entry = byEvent.get(key);
    if (!entry) {
      entry = {
        fleet_id: r.fleet_id,
        event_id: r.event_id,
        posture: r.posture,
        model: r.model,
        recorded_at: r.recorded_at ?? null,
        receive_nanos: "0",
        stage_nanos: "0",
        token_count_input: null,
        token_count_output: null,
        total_nanos: "0",
      };
      byEvent.set(key, entry);
    }
    applyCharge(entry, r);
    // The receive row is recorded first; pin recorded_at to the earliest of
    // the two so sort-by-time matches the event's gate-pass moment.
    if (
      r.recorded_at != null &&
      (entry.recorded_at == null || byInstant(r.recorded_at, entry.recorded_at) < 0)
    ) {
      entry.recorded_at = r.recorded_at;
    }
  }
  return Array.from(byEvent.values()).sort(
    (a, b) => (instantMs(b.recorded_at) ?? 0) - (instantMs(a.recorded_at) ?? 0),
  );
};

const renderActivities = (output: Output, events: ReadonlyArray<EventSummary>) =>
  output.printEntityTable(
    {
      id: { key: "event_id", label: "WORK_ID" },
      domain: [
        { key: "posture", label: "POSTURE" },
        { key: "model", label: "MODEL" },
        { key: "in_tok", label: "IN_TOK" },
        { key: "out_tok", label: "OUT_TOK" },
        { key: "receive", label: "RECEIVE" },
        { key: "stage", label: "STAGE" },
        { key: "setup", label: "SETUP" },
        { key: "total", label: "TOTAL" },
      ],
      // The table groups several ledger rows under one work identifier.
      ageKey: null,
    },
    events.map((e) => ({
      event_id: e.authoring_attempt_id ?? e.event_id ?? "",
      posture: e.posture ?? "",
      model: e.model ?? "",
      in_tok: e.token_count_input != null ? String(e.token_count_input) : EMPTY_CELL,
      out_tok: e.token_count_output != null ? String(e.token_count_output) : EMPTY_CELL,
      receive: formatDollars(e.receive_nanos),
      stage: formatDollars(e.stage_nanos),
      setup: formatDollars(e.setup_nanos ?? "0"),
      total: formatDollars(e.total_nanos),
    })),
  );

const renderHuman = (
  balance: BillingSnapshot,
  events: ReadonlyArray<EventSummary>,
  nextCursor: string | null,
): Effect.Effect<void, never, Output> =>
  Effect.gen(function* () {
    const output = yield* Output;
    const balanceNanos = balance.balance_nanos;
    yield* output.info(`Tenant balance:    ${formatDollars(balanceNanos)}`);
    yield* output.info("");

    if (events.length === 0) {
      yield* output.info("No billable activity recorded yet.");
    } else {
      yield* output.info(`Last ${events.length} activities drained credits:`);
      yield* renderActivities(output, events);
    }

    if (nextCursor) {
      yield* output.info("");
      yield* output.info(
        `(more costs available — re-run with --cursor ${nextCursor})`,
      );
    }
    yield* output.info("");
    if (balance.is_exhausted) {
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
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;

    const limit = yield* parseLimit(args.limit);
    const cursor = yield* parseCursor(args.cursor);

    // Limit the raw page and keep every returned row. Overfetch followed by
    // slicing grouped rows would discard setup costs before the next cursor.
    const chargesPath = cursor
      ? `${CHARGES_PATH}?limit=${limit}&cursor=${encodeURIComponent(cursor)}`
      : `${CHARGES_PATH}?limit=${limit}`;
    const [billing, charges] = yield* Effect.all(
      [
        http.request<unknown>({ path: TENANT_BILLING_PATH, token }).pipe(Effect.flatMap(decodeBilling)),
        http.request<unknown>({ path: chargesPath, token }).pipe(Effect.flatMap(decodeCharges)),
      ],
      { concurrency: 2 },
    );

    const events = groupRowsByEvent(charges.items);
    const nextCursor = charges.next_cursor ?? null;

    if (output.format !== OUTPUT_FORMAT.text) {
      yield* output.success(BILLING_SHOWN, {
        balance_nanos: billing.balance_nanos,
        is_exhausted: billing.is_exhausted,
        events,
        next_cursor: nextCursor,
      });
      return;
    }
    yield* renderHuman(billing, events, nextCursor);
  });
