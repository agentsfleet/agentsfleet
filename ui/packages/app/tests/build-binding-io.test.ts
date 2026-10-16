import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { readProcessRecord, RECORD_MAX_BYTES } from "./e2e/acceptance/fixtures/build-binding-io";

let directory: string;
let record: string;
beforeEach(async () => {
  directory = await mkdtemp(join(tmpdir(), "agentsfleet-process-proof-"));
  record = join(directory, "process.json");
  vi.stubEnv("ACCEPTANCE_RUNNER_PROCESS_EVIDENCE", record);
});
afterEach(async () => {vi.unstubAllEnvs(); await rm(directory, {recursive: true, force: true});});

describe("bounded process evidence", () => {
  it("requires an explicitly selected trusted process record", async () => {
    vi.stubEnv("ACCEPTANCE_RUNNER_PROCESS_EVIDENCE", "");
    await expect(readProcessRecord()).rejects.toThrow("must name a fresh trusted host process record");
  });
  it("accepts a complete record exactly at the byte ceiling", async () => {
    await writeFile(record, `{}${" ".repeat(RECORD_MAX_BYTES - 2)}`);
    expect(await readProcessRecord()).toEqual({});
  });
  it("refuses one byte over the ceiling before parsing private contents", async () => {
    await writeFile(record, "private".padEnd(RECORD_MAX_BYTES + 1, "x"));
    await expect(readProcessRecord()).rejects.toThrow("exceeds its bound");
  });
  it("preserves malformed JSON as a failed observation", async () => {
    await writeFile(record, "{");
    await expect(readProcessRecord()).rejects.toThrow(SyntaxError);
  });
  it("preserves a missing record as a failed observation", async () => {
    await expect(readProcessRecord()).rejects.toMatchObject({code: "ENOENT"});
  });
});
