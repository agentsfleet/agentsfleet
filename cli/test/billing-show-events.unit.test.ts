// `billing show` — charge rows folded into events. Receive and stage rows
// share an event_id and must render as one row; this is the only part of the
// command with arithmetic in it.

import { describe, test, expect } from "bun:test";
import { Effect } from "effect";
import { billingShowEffectFromArgs } from "../src/commands/billing.ts";
import { OUTPUT_FORMAT } from "../src/services/output.ts";
import { ServerError } from "../src/errors/index.ts";
import {
  BILLING_PATH,
  ONE_CENT_NANOS,
  TEST_CHARGE_NANOS,
  makeRecorder,
  outputLayer,
  credentialsLayer,
  httpClientLayer,
  configLayer,
  runWith,
  RECEIVE_ROW,
  STAGE_ROW,
} from "./helpers-billing-effect.ts";

describe("billingShowEffectFromArgs — grouped events and pagination", () => {
  test("groups receive+stage rows by event_id and emits the table", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: undefined,
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === BILLING_PATH) {
            return Effect.succeed({
              balance_nanos: 500 * ONE_CENT_NANOS,
              is_exhausted: false,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({
            items: [STAGE_ROW, RECEIVE_ROW],
          }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    const out = rec.stdout.join("\n");
    expect(out).toMatch(/Last 1 events drained credits:/);
    expect(out).toMatch(/TABLE:1/);
  });

  test("--json emits balance + grouped events + next_cursor", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: undefined,
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === BILLING_PATH) {
            return Effect.succeed({
              balance_nanos: 250 * ONE_CENT_NANOS,
              is_exhausted: false,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({
            items: [RECEIVE_ROW, STAGE_ROW],
            next_cursor: "tok_for_page_2",
          }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
    );
    await runWith(program);
    const body = JSON.parse(rec.stdout[0] ?? "{}") as {
      balance_nanos: number;
      is_exhausted: boolean;
      events: Array<{
        event_id: string;
        receive_nanos: number;
        stage_nanos: number;
        total_nanos: number;
        token_count_input: number;
        token_count_output: number;
      }>;
      next_cursor: string | null;
    };
    expect(body.balance_nanos).toBe(250 * ONE_CENT_NANOS);
    expect(body.is_exhausted).toBe(false);
    expect(body.events).toHaveLength(1);
    const ev = body.events[0];
    if (!ev) throw new Error("expected grouped event");
    expect(ev.event_id).toBe("evt_1");
    expect(ev.receive_nanos).toBe(ONE_CENT_NANOS);
    expect(ev.stage_nanos).toBe(2 * ONE_CENT_NANOS);
    expect(ev.total_nanos).toBe(3 * ONE_CENT_NANOS);
    expect(ev.token_count_input).toBe(820);
    expect(ev.token_count_output).toBe(1040);
    expect(body.next_cursor).toBe("tok_for_page_2");
  });

  test("--limit slices grouped events not raw rows", async () => {
    const rec = makeRecorder();
    const items: Array<Record<string, unknown>> = [];
    for (const eid of ["evt_a", "evt_b", "evt_c"]) {
      items.push({ ...RECEIVE_ROW, event_id: eid, recorded_at: items.length });
      items.push({ ...STAGE_ROW, event_id: eid, recorded_at: items.length });
    }
    const program = billingShowEffectFromArgs({
      limit: "2",
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === BILLING_PATH) {
            return Effect.succeed({
              balance_nanos: TEST_CHARGE_NANOS * ONE_CENT_NANOS,
              is_exhausted: false,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({ items }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
    );
    await runWith(program);
    const body = JSON.parse(rec.stdout[0] ?? "{}") as {
      events: Array<unknown>;
    };
    expect(body.events).toHaveLength(2);
  });

  test("surfaces next_cursor in text mode footer", async () => {
    const rec = makeRecorder();
    const program = billingShowEffectFromArgs({
      limit: undefined,
      cursor: undefined,
    }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer()),
      Effect.provide(
        httpClientLayer((path) => {
          if (path === BILLING_PATH) {
            return Effect.succeed({
              balance_nanos: 100 * ONE_CENT_NANOS,
              is_exhausted: false,
            }) as Effect.Effect<unknown, ServerError>;
          }
          return Effect.succeed({
            items: [RECEIVE_ROW, STAGE_ROW],
            next_cursor: "next_token_xyz",
          }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout.join("\n")).toMatch(
      /more events available — re-run with --cursor next_token_xyz/,
    );
  });
});
