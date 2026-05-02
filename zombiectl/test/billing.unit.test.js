import { test } from "bun:test";
import assert from "node:assert/strict";
import { commandBilling } from "../src/commands/billing.js";
import { makeNoop, makeBufferStream, ui } from "./helpers.js";

const BILLING_PATH = "/v1/tenants/me/billing";
const CHARGES_PATH_PREFIX = "/v1/tenants/me/billing/charges";

function makeDeps({ requestImpl, parseFlagsImpl } = {}) {
  return {
    parseFlags: parseFlagsImpl ?? ((tokens) => {
      const options = {};
      const positionals = [];
      for (let i = 0; i < tokens.length; i += 1) {
        const t = tokens[i];
        if (t.startsWith("--")) {
          const key = t.slice(2);
          const next = tokens[i + 1];
          if (next && !next.startsWith("--")) { options[key] = next; i += 1; }
          else options[key] = true;
        } else positionals.push(t);
      }
      return { options, positionals };
    }),
    request: requestImpl ?? (async () => ({})),
    apiHeaders: () => ({ Authorization: "Bearer token" }),
    ui,
    printJson: (stream, obj) => stream.write(JSON.stringify(obj)),
    printTable: (stream, _cols, rows) => stream.write(`TABLE:${rows.length}\n`),
    writeLine: (stream, line) => { if (stream && line !== undefined) stream.write(`${line}\n`); else if (stream) stream.write("\n"); },
  };
}

const RECEIVE_ROW = {
  event_id: "evt_1", charge_type: "receive", posture: "platform", model: "kimi-k2.6",
  credit_deducted_cents: 1, token_count_input: null, token_count_output: null,
  recorded_at: 1_000_000,
};
const STAGE_ROW = {
  event_id: "evt_1", charge_type: "stage", posture: "platform", model: "kimi-k2.6",
  credit_deducted_cents: 2, token_count_input: 820, token_count_output: 1040,
  recorded_at: 1_000_005,
};

// ── routing / unknown action ─────────────────────────────────────────────────

test("billing without action: prints usage and exits 2", async () => {
  const stderr = makeBufferStream();
  const ctx = { stdout: makeNoop(), stderr: stderr.stream, jsonMode: false };
  const code = await commandBilling(ctx, [], null, makeDeps());
  assert.equal(code, 2);
  assert.match(stderr.read(), /usage: zombiectl billing show/);
  assert.match(stderr.read(), /--cursor TOKEN/);
});

test("billing unknown action: --json emits UNKNOWN_COMMAND error", async () => {
  const stderr = makeBufferStream();
  const ctx = { stdout: makeNoop(), stderr: stderr.stream, jsonMode: true };
  const code = await commandBilling(ctx, ["bogus"], null, makeDeps());
  assert.equal(code, 2);
  const body = JSON.parse(stderr.read());
  assert.equal(body.error.code, "UNKNOWN_COMMAND");
});

// ── billing show: API contract ──────────────────────────────────────────────

test("billing show: GETs balance + usage in parallel, default limit=10 → usage limit=20", async () => {
  const calls = [];
  const deps = makeDeps({
    requestImpl: async (_ctx, url, opts) => {
      calls.push({ url, method: opts.method });
      if (url === BILLING_PATH) return { balance_cents: 471, is_exhausted: false };
      return { items: [] };
    },
  });
  const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false };
  const code = await commandBilling(ctx, ["show"], null, deps);
  assert.equal(code, 0);
  const urls = calls.map((c) => c.url).sort();
  assert.deepEqual(urls, [BILLING_PATH, `${CHARGES_PATH_PREFIX}?limit=20`]);
});

test("billing show --limit 5: requests usage with limit=10 (limit*2)", async () => {
  let usageUrl = null;
  const deps = makeDeps({
    requestImpl: async (_ctx, url) => {
      if (url.startsWith(CHARGES_PATH_PREFIX)) usageUrl = url;
      if (url === BILLING_PATH) return { balance_cents: 100, is_exhausted: false };
      return { items: [] };
    },
  });
  const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false };
  const code = await commandBilling(ctx, ["show", "--limit", "5"], null, deps);
  assert.equal(code, 0);
  assert.equal(usageUrl, `${CHARGES_PATH_PREFIX}?limit=10`);
});

test("billing show: rejects --limit 0 with exit 2", async () => {
  const stderr = makeBufferStream();
  const ctx = { stdout: makeNoop(), stderr: stderr.stream, jsonMode: false };
  const code = await commandBilling(ctx, ["show", "--limit", "0"], null, makeDeps());
  assert.equal(code, 2);
  assert.match(stderr.read(), /--limit must be an integer between 1 and 100/);
});

test("billing show: rejects non-numeric --limit", async () => {
  const stderr = makeBufferStream();
  const ctx = { stdout: makeNoop(), stderr: stderr.stream, jsonMode: false };
  const code = await commandBilling(ctx, ["show", "--limit", "lots"], null, makeDeps());
  assert.equal(code, 2);
  assert.match(stderr.read(), /--limit must be an integer between 1 and 100/);
});

test("billing show: rejects bare --limit (no value) with usage hint", async () => {
  const stderr = makeBufferStream();
  const ctx = { stdout: makeNoop(), stderr: stderr.stream, jsonMode: false };
  // parseFlags treats `--limit` followed by `--json` (a flag) as boolean true.
  const code = await commandBilling(ctx, ["show", "--limit", "--json"], null, makeDeps());
  assert.equal(code, 2);
  assert.match(stderr.read(), /--limit requires a value/);
});

test("billing show: rejects bare --cursor (no value)", async () => {
  const stderr = makeBufferStream();
  const ctx = { stdout: makeNoop(), stderr: stderr.stream, jsonMode: false };
  const code = await commandBilling(ctx, ["show", "--cursor"], null, makeDeps());
  assert.equal(code, 2);
  assert.match(stderr.read(), /--cursor requires a value/);
});

test("billing show: rejects empty --cursor with JSON error code", async () => {
  const stderr = makeBufferStream();
  const ctx = { stdout: makeNoop(), stderr: stderr.stream, jsonMode: true };
  // Use the real parseFlags so `--cursor=` resolves to options.cursor="".
  const { parseFlags: realParseFlags } = await import("../src/program/args.js");
  const code = await commandBilling(
    ctx,
    ["show", "--cursor=", "--json"],
    null,
    makeDeps({ parseFlagsImpl: realParseFlags }),
  );
  assert.equal(code, 2);
  assert.equal(JSON.parse(stderr.read()).error.code, "INVALID_CURSOR");
});

test("billing show: rejects --limit above max with JSON error code", async () => {
  const stderr = makeBufferStream();
  const ctx = { stdout: makeNoop(), stderr: stderr.stream, jsonMode: true };
  const code = await commandBilling(ctx, ["show", "--limit", "9999"], null, makeDeps());
  assert.equal(code, 2);
  assert.equal(JSON.parse(stderr.read()).error.code, "INVALID_LIMIT");
});

// ── billing show: text-mode rendering ───────────────────────────────────────

test("billing show: text mode prints formatted balance and footer pointer", async () => {
  const stdout = makeBufferStream();
  const deps = makeDeps({
    requestImpl: async (_ctx, url) => {
      if (url === BILLING_PATH) return { balance_cents: 471, is_exhausted: false };
      return { items: [] };
    },
  });
  const ctx = { stdout: stdout.stream, stderr: makeNoop(), jsonMode: false };
  await commandBilling(ctx, ["show"], null, deps);
  const out = stdout.read();
  assert.match(out, /Tenant balance:    \$4\.71 \(471¢\)/);
  assert.match(out, /No billable events recorded yet\./);
  assert.match(out, /Out of credits\? See https:\/\/app\.usezombie\.com\/settings\/billing/);
});

test("billing show: surfaces exhausted balance with explicit warning", async () => {
  const stdout = makeBufferStream();
  const deps = makeDeps({
    requestImpl: async (_ctx, url) => {
      if (url === BILLING_PATH) return { balance_cents: 0, is_exhausted: true };
      return { items: [] };
    },
  });
  const ctx = { stdout: stdout.stream, stderr: makeNoop(), jsonMode: false };
  await commandBilling(ctx, ["show"], null, deps);
  assert.match(stdout.read(), /⚠ Out of credits\. See https:\/\/app\.usezombie\.com/);
});

test("billing show: groups receive+stage rows by event_id and sums totals", async () => {
  const stdout = makeBufferStream();
  const deps = makeDeps({
    requestImpl: async (_ctx, url) => {
      if (url === BILLING_PATH) return { balance_cents: 500, is_exhausted: false };
      return { items: [STAGE_ROW, RECEIVE_ROW] }; // out-of-order on purpose
    },
  });
  const ctx = { stdout: stdout.stream, stderr: makeNoop(), jsonMode: false };
  await commandBilling(ctx, ["show"], null, deps);
  const out = stdout.read();
  assert.match(out, /Last 1 events drained credits:/);
  assert.match(out, /TABLE:1/);
});

// ── billing show: JSON-mode contract ────────────────────────────────────────

test("billing show --json: emits balance + grouped events array", async () => {
  const stdout = makeBufferStream();
  const deps = makeDeps({
    requestImpl: async (_ctx, url) => {
      if (url === BILLING_PATH) return { balance_cents: 250, is_exhausted: false };
      return { items: [RECEIVE_ROW, STAGE_ROW] };
    },
  });
  const ctx = { stdout: stdout.stream, stderr: makeNoop(), jsonMode: true };
  const code = await commandBilling(ctx, ["show", "--json"], null, deps);
  assert.equal(code, 0);
  const body = JSON.parse(stdout.read());
  assert.equal(body.balance_cents, 250);
  assert.equal(body.is_exhausted, false);
  assert.equal(body.events.length, 1);
  const ev = body.events[0];
  assert.equal(ev.event_id, "evt_1");
  assert.equal(ev.receive_cents, 1);
  assert.equal(ev.stage_cents, 2);
  assert.equal(ev.total_cents, 3);
  assert.equal(ev.token_count_input, 820);
  assert.equal(ev.token_count_output, 1040);
});

test("billing show: forwards --cursor verbatim (URI-encoded) to the charges endpoint", async () => {
  const calls = [];
  const deps = makeDeps({
    requestImpl: async (_ctx, url) => {
      calls.push(url);
      if (url === BILLING_PATH) return { balance_cents: 100, is_exhausted: false };
      return { items: [], next_cursor: null };
    },
  });
  const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false };
  await commandBilling(ctx, ["show", "--cursor", "abc/=def"], null, deps);
  const usageUrl = calls.find((u) => u.startsWith(CHARGES_PATH_PREFIX));
  assert.match(usageUrl, /[?&]cursor=abc%2F%3Ddef/);
});

test("billing show: surfaces next_cursor in text mode footer when present", async () => {
  const stdout = makeBufferStream();
  const deps = makeDeps({
    requestImpl: async (_ctx, url) => {
      if (url === BILLING_PATH) return { balance_cents: 100, is_exhausted: false };
      return { items: [RECEIVE_ROW, STAGE_ROW], next_cursor: "next_token_xyz" };
    },
  });
  const ctx = { stdout: stdout.stream, stderr: makeNoop(), jsonMode: false };
  await commandBilling(ctx, ["show"], null, deps);
  assert.match(stdout.read(), /more events available — re-run with --cursor next_token_xyz/);
});

test("billing show --json: surfaces next_cursor in the body for scripting", async () => {
  const stdout = makeBufferStream();
  const deps = makeDeps({
    requestImpl: async (_ctx, url) => {
      if (url === BILLING_PATH) return { balance_cents: 100, is_exhausted: false };
      return { items: [], next_cursor: "tok_for_page_2" };
    },
  });
  const ctx = { stdout: stdout.stream, stderr: makeNoop(), jsonMode: true };
  await commandBilling(ctx, ["show", "--json"], null, deps);
  const body = JSON.parse(stdout.read());
  assert.equal(body.next_cursor, "tok_for_page_2");
});

test("billing show --json: limit slices grouped events not raw rows", async () => {
  const stdout = makeBufferStream();
  // Three events, two rows each → 6 raw rows. limit=2 should yield 2 events.
  const items = [];
  for (const eid of ["evt_a", "evt_b", "evt_c"]) {
    items.push({ ...RECEIVE_ROW, event_id: eid, recorded_at: items.length });
    items.push({ ...STAGE_ROW,   event_id: eid, recorded_at: items.length });
  }
  const deps = makeDeps({
    requestImpl: async (_ctx, url) => {
      if (url === BILLING_PATH) return { balance_cents: 1000, is_exhausted: false };
      return { items };
    },
  });
  const ctx = { stdout: stdout.stream, stderr: makeNoop(), jsonMode: true };
  await commandBilling(ctx, ["show", "--limit", "2", "--json"], null, deps);
  const body = JSON.parse(stdout.read());
  assert.equal(body.events.length, 2);
});
