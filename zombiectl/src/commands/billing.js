// Read-only `zombiectl billing show [--limit N] [--cursor TOKEN] [--json]`.
//
// Calls GET /v1/tenants/me/billing for the balance card and GET
// /v1/tenants/me/billing/charges for the per-event drain history. Each event
// produces up to two charge rows (charge_type ∈ {receive, stage}); the CLI
// groups them by event_id so each row in the table represents one event with
// both charges combined. `--json` emits the raw shape for scripting and
// includes `next_cursor` so callers can paginate.

import { writeError } from "../program/io.js";

const BILLING_PATH = "/v1/tenants/me/billing";
const CHARGES_PATH = "/v1/tenants/me/billing/charges";
const BILLING_DASHBOARD_URL = "https://app.usezombie.com/settings/billing";
const PURCHASE_FOOTER_LINE_2 = "Stripe purchase ships in v2.1; for now contact support for a top-up.";

const DEFAULT_LIMIT = 10;
const MAX_LIMIT = 100;

export async function commandBilling(ctx, args, _workspaces, deps) {
  const { parseFlags, ui, writeLine } = deps;
  const action = args[0];
  const parsed = parseFlags(args.slice(1));

  if (action === "show") return commandBillingShow(ctx, parsed, deps);

  if (ctx.jsonMode) {
    writeError(ctx, "UNKNOWN_COMMAND", `unknown billing action: ${action ?? "(none)"}`, deps);
  } else {
    writeLine(ctx.stderr, ui.err("usage: zombiectl billing show [--limit N] [--cursor TOKEN] [--json]"));
  }
  return 2;
}

export async function commandBillingShow(ctx, parsed, deps) {
  const { request, apiHeaders, ui, printJson, printTable, writeLine } = deps;

  const limit = parseLimitOption(parsed.options.limit);
  if (limit instanceof Error) {
    if (ctx.jsonMode) {
      writeError(ctx, "INVALID_LIMIT", limit.message, deps);
    } else {
      writeLine(ctx.stderr, ui.err(limit.message));
    }
    return 2;
  }

  // Fetch balance + charges in parallel. Charges limit is `limit * 2` because
  // each event has up to 2 rows (receive + stage); after grouping we slice to
  // the user-requested event count. `--cursor` is forwarded verbatim — the
  // server treats it as opaque.
  const cursor = parsed.options.cursor;
  const chargesQs = cursor
    ? `${CHARGES_PATH}?limit=${limit * 2}&cursor=${encodeURIComponent(String(cursor))}`
    : `${CHARGES_PATH}?limit=${limit * 2}`;
  const [billing, charges] = await Promise.all([
    request(ctx, BILLING_PATH, { method: "GET", headers: apiHeaders(ctx) }),
    request(ctx, chargesQs, { method: "GET", headers: apiHeaders(ctx) }),
  ]);

  const events = groupRowsByEvent(charges?.items ?? []).slice(0, limit);
  const nextCursor = charges?.next_cursor ?? null;

  if (ctx.jsonMode) {
    printJson(ctx.stdout, {
      balance_cents: billing.balance_cents ?? 0,
      is_exhausted: Boolean(billing.is_exhausted),
      events,
      next_cursor: nextCursor,
    });
    return 0;
  }

  const balanceCents = billing.balance_cents ?? 0;
  const balanceDollars = (balanceCents / 100).toFixed(2);
  writeLine(ctx.stdout, `Tenant balance:    $${balanceDollars} (${balanceCents}¢)`);
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
      event_id: e.event_id,
      posture: e.posture,
      model: e.model,
      in_tok: e.token_count_input != null ? String(e.token_count_input) : "—",
      out_tok: e.token_count_output != null ? String(e.token_count_output) : "—",
      receive: `${e.receive_cents}¢`,
      stage: `${e.stage_cents}¢`,
      total: `${e.total_cents}¢`,
    })));
  }

  if (nextCursor) {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.dim(`(more events available — re-run with --cursor ${nextCursor})`));
  }
  writeLine(ctx.stdout);
  if (billing.is_exhausted) {
    writeLine(ctx.stdout, ui.err(`⚠ Out of credits. See ${BILLING_DASHBOARD_URL}`));
  } else {
    writeLine(ctx.stdout, ui.dim(`ⓘ Out of credits? See ${BILLING_DASHBOARD_URL}`));
  }
  writeLine(ctx.stdout, ui.dim(`   ${PURCHASE_FOOTER_LINE_2}`));
  return 0;
}

function parseLimitOption(raw) {
  if (raw === undefined || raw === null) return DEFAULT_LIMIT;
  const n = Number.parseInt(String(raw), 10);
  if (!Number.isFinite(n) || n <= 0 || n > MAX_LIMIT) {
    return new Error(`--limit must be an integer between 1 and ${MAX_LIMIT}`);
  }
  return n;
}

function groupRowsByEvent(rows) {
  const byEvent = new Map();
  for (const r of rows) {
    let entry = byEvent.get(r.event_id);
    if (!entry) {
      entry = {
        event_id: r.event_id,
        posture: r.posture,
        model: r.model,
        recorded_at: r.recorded_at,
        receive_cents: 0,
        stage_cents: 0,
        token_count_input: null,
        token_count_output: null,
        total_cents: 0,
      };
      byEvent.set(r.event_id, entry);
    }
    if (r.charge_type === "receive") {
      entry.receive_cents = r.credit_deducted_cents ?? 0;
    } else if (r.charge_type === "stage") {
      entry.stage_cents = r.credit_deducted_cents ?? 0;
      entry.token_count_input = r.token_count_input;
      entry.token_count_output = r.token_count_output;
    }
    // The receive row is recorded first; pin recorded_at to the earliest of
    // the two so sort-by-time matches the event's gate-pass moment.
    if (r.recorded_at != null && (entry.recorded_at == null || r.recorded_at < entry.recorded_at)) {
      entry.recorded_at = r.recorded_at;
    }
    entry.total_cents = entry.receive_cents + entry.stage_cents;
  }
  return Array.from(byEvent.values()).sort((a, b) => (b.recorded_at ?? 0) - (a.recorded_at ?? 0));
}
