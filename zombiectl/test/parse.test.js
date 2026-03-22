import { test } from "bun:test";
import assert from "node:assert/strict";
import { parseGlobalArgs } from "../src/cli.js";

test("parseGlobalArgs uses flag api over env", () => {
  const env = { ZOMBIE_API_URL: "https://env.example" };
  const out = parseGlobalArgs(["--api", "https://flag.example", "doctor"], env);
  assert.equal(out.global.apiUrl, "https://flag.example");
  assert.equal(out.rest[0], "doctor");
});

test("parseGlobalArgs falls back to env api", () => {
  const env = { ZOMBIE_API_URL: "https://env.example" };
  const out = parseGlobalArgs(["doctor"], env);
  assert.equal(out.global.apiUrl, "https://env.example");
});
