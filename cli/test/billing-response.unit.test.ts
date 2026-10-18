import { expect, test } from "bun:test";
import { Effect, Exit } from "effect";
import { billingShowEffectFromArgs } from "../src/commands/billing.ts";
import { OUTPUT_FORMAT } from "../src/services/output.ts";
import {
  BILLING_PATH, configLayer, credentialsLayer, expectFailure, httpClientLayer,
  makeRecorder, outputLayer, RECEIVE_ROW, STAGE_ROW, runWith,
} from "./helpers-billing-effect.ts";

const run = async (wallet: unknown, charges: unknown) => {
  const rec = makeRecorder();
  const exit = await runWith(billingShowEffectFromArgs({ limit: undefined, cursor: undefined }).pipe(
    Effect.provide(configLayer()), Effect.provide(credentialsLayer()),
    Effect.provide(httpClientLayer(path => Effect.succeed(path === BILLING_PATH ? wallet : charges), rec)),
    Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
  ));
  return { rec, exit };
};
const wallet = { balance_nanos: "9223372036854775807", is_exhausted: false };

test.each([1, null, undefined, "01", "-1", "1.5", "9223372036854775808"])(
  "a malformed wallet %j cannot masquerade as a zero balance", async balance_nanos => {
    const { exit, rec } = await run({ ...wallet, balance_nanos }, { items: [] });
    expect(expectFailure(exit)).toMatchObject({ code: "BILLING_RESPONSE_INVALID", status: 502 });
    expect(rec.stdout).toEqual([]);
  },
);
test.each([1, null, undefined, "01", "-1", "1e3", "9223372036854775808"])(
  "a malformed charge %j cannot disappear from a bill", async credit_deducted_nanos => {
    const { exit, rec } = await run(wallet, { items: [{ ...RECEIVE_ROW, credit_deducted_nanos }] });
    expect(expectFailure(exit)).toMatchObject({ code: "BILLING_RESPONSE_INVALID" });
    expect(rec.stdout).toEqual([]);
  },
);
test("repeated ledger categories add exactly and future categories remain in the total", async () => {
  const { exit, rec } = await run(wallet, { items: [
    { ...STAGE_ROW, credit_deducted_nanos: "9007199254740993", token_count_input: 20, token_count_output: null },
    { ...STAGE_ROW, credit_deducted_nanos: "1", token_count_input: 3, token_count_output: 5 },
    { ...RECEIVE_ROW, credit_deducted_nanos: "7" },
    { ...RECEIVE_ROW, charge_type: "future_fee", credit_deducted_nanos: "2" },
  ], next_cursor: null });
  expect(Exit.isSuccess(exit)).toBe(true);
  expect(JSON.parse(rec.stdout[0] ?? "{}")).toMatchObject({
    balance_nanos: "9223372036854775807", events: [{ receive_nanos: "7",
      stage_nanos: "9007199254740994", total_nanos: "9007199254741003",
      token_count_input: 23, token_count_output: 5 }], next_cursor: null,
  });
});
test("an incomplete charge page cannot be reported as no activity", async () => {
  const { exit, rec } = await run(wallet, {});
  expect(expectFailure(exit)).toMatchObject({ code: "BILLING_RESPONSE_INVALID" });
  expect(rec.stdout).toEqual([]);
});
