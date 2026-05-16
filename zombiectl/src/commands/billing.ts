// Read-only `zombiectl billing show [--limit N] [--cursor TOKEN] [--json]`.
//
// Calls GET /v1/tenants/me/billing for the balance card and GET
// /v1/tenants/me/billing/charges for the per-event drain history. Each event
// produces up to two charge rows (charge_type ∈ {receive, stage}); the CLI
// groups them by event_id so each row in the table represents one event with
// both charges combined. `--json` emits the raw shape for scripting and
// includes `next_cursor` so callers can paginate.

import { AUTH_PRESET } from "../lib/error-map-presets.ts";
import { writeError as ioWriteError } from "../program/io.ts";
import { CHARGE_TYPE, formatDollars } from "../constants/billing.ts";
import { TENANT_BILLING_PATH } from "../lib/api-paths.ts";
import type {
  CommandCtx,
  CommandDeps,
  ParsedArgs,
  Workspaces,
} from "./types.ts";

// Billing show hits /v1/tenants/me/billing (GET) + charges (GET).
// Auth-only surface; the server propagates UZ-BILLING-* internally
// but the CLI's read endpoint surfaces them as plain server messages.
export const errorMap = AUTH_PRESET;

const BILLING_PATH = TENANT_BILLING_PATH;
const CHARGES_PATH = `${TENANT_BILLING_PATH}/charges`;
const BILLING_DASHBOARD_URL = "https://app.usezombie.com/settings/billing";
const PURCHASE_FOOTER_LINE_2 = "Stripe purchase ships in v2.1; for now contact support for a top-up.";
const INVALID_LIMIT = "INVALID_LIMIT";
const INVALID_CURSOR = "INVALID_CURSOR";

const DEFAULT_LIMIT = 10;
const MAX_LIMIT = 100;

interface BillingSnapshot {
  balance_nanos?: number;
  is_exhausted?: boolean;
  [key: string]: unknown;
}

interface ChargeRow {
  event_id?: string;
  posture?: string;
  model?: string;
  recorded_at?: number | null;
  charge_type?: string;
  credit_deducted_nanos?: number;
  token_count_input?: number | null;
  token_count_output?: number | null;
}

interface ChargesResponse {
  items?: ChargeRow[];
  next_cursor?: string | null;
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

export async function commandBillingShow(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  _workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, printTable, writeLine } = deps;

  const limit = parseLimitOption(parsed.options["limit"]);
  if (limit instanceof Error) {
    if (ctx.jsonMode) {
      ioWriteError(ctx, INVALID_LIMIT, limit.message, deps);
    } else if (ctx.stderr) {
      writeLine(ctx.stderr, ui.err(limit.message));
    }
    return 2;
  }

  const cursor = parseCursorOption(parsed.options["cursor"]);
  if (cursor instanceof Error) {
    if (ctx.jsonMode) {
      ioWriteError(ctx, INVALID_CURSOR, cursor.message, deps);
    } else if (ctx.stderr) {
      writeLine(ctx.stderr, ui.err(cursor.message));
    }
    return 2;
  }

  // Fetch balance + charges in parallel. Charges limit is `limit * 2` because
  // each event has up to 2 rows (receive + stage); after grouping we slice to
  // the user-requested event count. `--cursor` is forwarded verbatim — the
  // server treats it as opaque.
  const chargesQs = cursor
    ? `${CHARGES_PATH}?limit=${limit * 2}&cursor=${encodeURIComponent(cursor)}`
    : `${CHARGES_PATH}?limit=${limit * 2}`;
  const [billing, charges] = (await Promise.all([
    request(ctx, BILLING_PATH, { method: "GET", headers: apiHeaders(ctx) }),
    request(ctx, chargesQs, { method: "GET", headers: apiHeaders(ctx) }),
  ])) as [BillingSnapshot | null, ChargesResponse | null];

  const events = groupRowsByEvent(charges?.items ?? []).slice(0, limit);
  const nextCursor = charges?.next_cursor ?? null;

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, {
      balance_nanos: billing?.balance_nanos ?? 0,
      is_exhausted: Boolean(billing?.is_exhausted),
      events,
      next_cursor: nextCursor,
    });
    return 0;
  }

  if (!ctx.stdout) return 0;
  const balanceNanos = billing?.balance_nanos ?? 0;
  writeLine(ctx.stdout, `Tenant balance:    ${formatDollars(balanceNanos)}`);
  writeLine(ctx.stdout);

  if (events.length === 0) {
    writeLine(ctx.stdout, ui.dim("No billable events recorded yet."));
  } else {
    writeLine(ctx.stdout, `Last ${events.length} events drained credits:`);
    printTable(ctx.stdout, [
      { key: "event_id", label: "EVENT_ID" },
      { key: "posture",  label: "POSTURE" },
      { key: "model",    label: "MODEL" },
      { key: "in_tok",   label: "IN_TOK" },
      { key: "out_tok",  label: "OUT_TOK" },
      { key: "receive",  label: "RECEIVE" },
      { key: "stage",    label: "STAGE" },
      { key: "total",    label: "TOTAL" },
    ], events.map((e) => ({
      event_id: e.event_id ?? "",
      posture: e.posture ?? "",
      model: e.model ?? "",
      in_tok: e.token_count_input != null ? String(e.token_count_input) : "—",
      out_tok: e.token_count_output != null ? String(e.token_count_output) : "—",
      receive: formatDollars(e.receive_nanos),
      stage: formatDollars(e.stage_nanos),
      total: formatDollars(e.total_nanos),
    })));
  }

  if (nextCursor) {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.dim(`(more events available — re-run with --cursor ${nextCursor})`));
  }
  writeLine(ctx.stdout);
  if (billing?.is_exhausted) {
    writeLine(ctx.stdout, ui.err(`⚠ Out of credits. See ${BILLING_DASHBOARD_URL}`));
  } else {
    writeLine(ctx.stdout, ui.dim(`ⓘ Out of credits? See ${BILLING_DASHBOARD_URL}`));
  }
  writeLine(ctx.stdout, ui.dim(`   ${PURCHASE_FOOTER_LINE_2}`));
  return 0;
}

function parseLimitOption(
  raw: string | boolean | number | string[] | undefined | null,
): number | Error {
  if (raw === undefined || raw === null) return DEFAULT_LIMIT;
  // boolean true = bare `--limit` with no following value. Caller-shim
  // path; commander rejects this at parse-time in production.
  if (raw === true) return new Error("--limit requires a value (e.g. --limit 25)");
  const n = Number.parseInt(String(raw), 10);
  if (!Number.isFinite(n) || n <= 0 || n > MAX_LIMIT) {
    return new Error(`--limit must be an integer between 1 and ${MAX_LIMIT}`);
  }
  return n;
}

function parseCursorOption(
  raw: string | boolean | number | string[] | undefined | null,
): string | null | Error {
  if (raw === undefined || raw === null) return null;
  // boolean true = bare `--cursor` with no following value (caller-shim
  // path). Reject before URI-encoding the literal string "true".
  if (raw === true) return new Error("--cursor requires a value (the next_cursor token from a previous page)");
  const s = String(raw);
  if (s.length === 0) return new Error("--cursor must not be empty");
  return s;
}

function groupRowsByEvent(rows: ChargeRow[]): EventSummary[] {
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
    if (r.recorded_at != null && (entry.recorded_at == null || r.recorded_at < entry.recorded_at)) {
      entry.recorded_at = r.recorded_at;
    }
    entry.total_nanos = entry.receive_nanos + entry.stage_nanos;
  }
  return Array.from(byEvent.values()).sort((a, b) => (b.recorded_at ?? 0) - (a.recorded_at ?? 0));
}
