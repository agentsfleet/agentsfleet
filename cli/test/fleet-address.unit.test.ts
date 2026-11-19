// The address table the command line and the dashboard both replay. The
// composer's suite (ui/packages/app/lib/chat-address.test.ts) reads the same
// file; a case that passes there and fails here is a disagreement between the
// two parsers, which is what the table exists to catch.

import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import path from "node:path";
import { Schema } from "effect";
import { readFleetAddress } from "../src/lib/fleet-address.ts";

const FIXTURE_PATH = path.resolve(import.meta.dir, "..", "..", "tests", "fixtures", "fleet-address", "cases.json");

const AddressCase = Schema.Struct({ input: Schema.String, name: Schema.String, rest: Schema.String });
const PlainCase = Schema.Struct({ input: Schema.String, not_an_address: Schema.Literal(true) });
// A malformed case fails the decode, so the test goes red rather than
// skipping a row it could not read.
const AddressTable = Schema.Struct({
  schema_version: Schema.Literal(1),
  cases: Schema.Array(Schema.Union([AddressCase, PlainCase])),
});

const table = Schema.decodeUnknownSync(AddressTable)(JSON.parse(readFileSync(FIXTURE_PATH, "utf8")));

describe("the command line reads the shared address table", () => {
  test("fleet_address_reads_every_shared_case", () => {
    expect(table.cases.length).toBeGreaterThan(0);
    for (const row of table.cases) {
      const expected = "name" in row ? { name: row.name, rest: row.rest } : undefined;
      // The input rides along so a failure names the case that broke.
      expect({ input: row.input, address: readFleetAddress(row.input) }).toEqual({ input: row.input, address: expected });
    }
  });
});
