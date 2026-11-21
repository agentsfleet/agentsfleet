import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { z } from "zod";
import { parseChatAddress, readFleetHandoff } from "./chat-address";

// The case table the command line reads too (cli/test/fleet-address.unit.test.ts):
// a case that passes here and fails there is a disagreement between the two
// parsers, which is what the table exists to catch.

// Resolved from this file, so the suite reads the same table wherever vitest is run from.
const FIXTURE_PATH = path.resolve(__dirname, "../../../../tests/fixtures/fleet-address/cases.json");
const CaseSchema = z.union([
  z.object({ input: z.string(), name: z.string(), rest: z.string() }).strict(),
  z.object({ input: z.string(), not_an_address: z.literal(true) }).strict(),
]);
const FixtureSchema = z.object({ schema_version: z.literal(1), cases: z.array(CaseSchema).min(1) });
const { cases } = FixtureSchema.parse(JSON.parse(readFileSync(FIXTURE_PATH, "utf8")));

describe("parseChatAddress", () => {
  it("chat_address_reads_every_shared_case", () => {
    const read = cases.map((entry) => ({ input: entry.input, address: parseChatAddress(entry.input) }));
    const expected = cases.map((entry) => ({
      input: entry.input,
      address: "not_an_address" in entry ? null : { name: entry.name, rest: entry.rest },
    }));
    expect(read).toEqual(expected);
  });

  it("reads no address in empty or blank text, or in an address after the first word", () => {
    for (const input of ["", "   ", "\n\t", "retry\n@agentbob now"]) {
      expect(parseChatAddress(input)).toBeNull();
    }
  });

  it("drops the blank lines after the address and keeps every line of the message", () => {
    expect(parseChatAddress("@agentbob\n\n  retry run 991\nthen report")).toEqual({
      name: "agentbob",
      rest: "retry run 991\nthen report",
    });
  });
});

describe("readFleetHandoff", () => {
  const id = "0199a801-1000-7000-8000-0000000000c0";
  const message = `[from fleet chief-of-staff, fleet_id ${id}; answer with fleet_steer to that id]\nRun the check.\nReport back.`;

  it("keeps the sender and the complete message body", () => {
    expect(readFleetHandoff(`fleet:${id}`, message)).toEqual({name: "chief-of-staff", words: "Run the check.\nReport back."});
  });

  it("leaves ordinary messages and mismatched actor identities untouched", () => {
    expect(readFleetHandoff("steer:owner", message)).toBeNull();
    expect(readFleetHandoff("fleet:other", message)).toBeNull();
    expect(readFleetHandoff(`fleet:${id}`, "An ordinary report")).toBeNull();
  });
});
