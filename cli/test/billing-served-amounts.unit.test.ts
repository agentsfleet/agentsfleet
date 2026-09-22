// `billing show` — every money figure in the table comes off the wire.
//
// The command-line interface holds the nanos denominator and no rate, so the
// only arithmetic it may do is add the served per-row amounts. The rows below
// carry amounts no per-second rate could have produced against their token
// counts, which is what makes a recomputed figure visible if one ever returns.

import { describe, test, expect } from "bun:test";
import { Effect } from "effect";
import { billingShowEffectFromArgs } from "../src/commands/billing.ts";
import { ServerError } from "../src/errors/index.ts";
import {
  BILLING_PATH,
  ONE_CENT_NANOS,
  makeRecorder,
  outputLayer,
  credentialsLayer,
  httpClientLayer,
  configLayer,
  runWith,
  RECEIVE_ROW,
  STAGE_ROW,
} from "./helpers-billing-effect.ts";

const SERVED_RECEIVE_NANOS = 7_111_111;
const SERVED_STAGE_NANOS = 12_345_678;
const SERVED_RECEIVE_LABEL = "$0.0071";
const SERVED_STAGE_LABEL = "$0.0123";
const SERVED_TOTAL_LABEL = "$0.0195";

describe("billingShowEffectFromArgs — served amounts are rendered, not derived", () => {
  test("prints each row's own charge and their sum, with no rate arithmetic", async () => {
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
            items: [
              { ...RECEIVE_ROW, credit_deducted_nanos: SERVED_RECEIVE_NANOS },
              { ...STAGE_ROW, credit_deducted_nanos: SERVED_STAGE_NANOS },
            ],
          }) as Effect.Effect<unknown, ServerError>;
        }, rec),
      ),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    const [row] = rec.tables[0] ?? [];

    // The stage row reports 820 input and 1040 output tokens. Any client-side
    // pricing of those would land somewhere other than what the ledger charged.
    expect(row?.receive).toBe(SERVED_RECEIVE_LABEL);
    expect(row?.stage).toBe(SERVED_STAGE_LABEL);
    expect(row?.total).toBe(SERVED_TOTAL_LABEL);
  });
});
