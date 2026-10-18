import { describe, expect, test } from "bun:test";
import { Effect } from "effect";
import { billingShowEffectFromArgs } from "../src/commands/billing.ts";
import { CHARGE_TYPE } from "../src/constants/billing.ts";
import { OUTPUT_FORMAT } from "../src/services/output.ts";
import {
  BILLING_PATH, CHARGES_PATH_PREFIX, configLayer, credentialsLayer, httpClientLayer,
  makeRecorder, outputLayer, runWith,
} from "./helpers-billing-effect.ts";

const SETUP_ROWS = [
  { id: "settlement-1", authoring_attempt_id: "attempt-1", fleet_id: null, event_id: null,
    charge_type: CHARGE_TYPE.authoring, credit_deducted_nanos: 17_000_000,
    token_count_input: 23, token_count_output: 29, recorded_at: 101 },
  { id: "settlement-2", authoring_attempt_id: "attempt-2", fleet_id: null, event_id: null,
    charge_type: CHARGE_TYPE.authoring, credit_deducted_nanos: 19_000_000,
    token_count_input: 31, token_count_output: 37, recorded_at: 100 },
];

describe("billing setup costs", () => {
  test("does not skip paid setup attempts between server pages", async () => {
    const rec = makeRecorder();
    await runWith(billingShowEffectFromArgs({ limit: 1, cursor: undefined }).pipe(
      Effect.provide(configLayer()), Effect.provide(credentialsLayer()),
      Effect.provide(httpClientLayer(path => {
        if (path === BILLING_PATH) return Effect.succeed({ balance_nanos: 42_000_000 });
        const size = Number(new URL(path, "https://fixture.invalid").searchParams.get("limit"));
        return Effect.succeed({ items: SETUP_ROWS.slice(0, size), next_cursor: "cursor-after-returned-rows" });
      }, rec)), Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
    ));
    expect(rec.httpCalls).toContain(`${CHARGES_PATH_PREFIX}?limit=1`);
    expect(JSON.parse(rec.stdout[0] ?? "{}").events).toHaveLength(1);
  });

  test("keeps independent setup attempts and their actual debit in JSON", async () => {
    const rec = makeRecorder();
    const result = await runWith(billingShowEffectFromArgs({ limit: undefined, cursor: undefined }).pipe(
      Effect.provide(configLayer()), Effect.provide(credentialsLayer()),
      Effect.provide(httpClientLayer(path => Effect.succeed(path === BILLING_PATH
        ? { balance_nanos: 42_000_000 } : { items: SETUP_ROWS }), rec)),
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
    ));
    expect(result._tag).toBe("Success");
    const body = JSON.parse(rec.stdout[0] ?? "{}");
    expect(body.events).toHaveLength(2);
    expect(body.events.map((e: { total_nanos: number }) => e.total_nanos)).toEqual([17_000_000, 19_000_000]);
    expect(body.events.map((e: { authoring_attempt_id: string }) => e.authoring_attempt_id)).toEqual(["attempt-1", "attempt-2"]);
    expect(body.events[0].token_count_input).toBe(23);
    expect(body.events[1].token_count_output).toBe(37);
  });

  test("shows setup costs as setup rather than free fleet events", async () => {
    const rec = makeRecorder();
    const result = await runWith(billingShowEffectFromArgs({ limit: undefined, cursor: undefined }).pipe(
      Effect.provide(configLayer()), Effect.provide(credentialsLayer()),
      Effect.provide(httpClientLayer(path => Effect.succeed(path === BILLING_PATH
        ? { balance_nanos: 42_000_000 } : { items: SETUP_ROWS }), rec)),
      Effect.provide(outputLayer(rec)),
    ));
    expect(result._tag).toBe("Success");
    expect(rec.tables[0]?.map(r => r["setup"])).toEqual(["$0.017", "$0.019"]);
    expect(rec.tables[0]?.map(r => r["total"])).toEqual(["$0.017", "$0.019"]);
  });
});
