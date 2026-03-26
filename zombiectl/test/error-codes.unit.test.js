import { test } from "bun:test";
import assert from "node:assert/strict";
import { ERR_BILLING_CREDIT_EXHAUSTED } from "../src/constants/error-codes.js";

test("ERR_BILLING_CREDIT_EXHAUSTED matches canonical code", () => {
  assert.equal(ERR_BILLING_CREDIT_EXHAUSTED, "UZ-BILLING-005");
});
