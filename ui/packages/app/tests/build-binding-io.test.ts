import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { readAppBuild, readProcessRecord, RECORD_MAX_BYTES } from "./e2e/acceptance/fixtures/build-binding-io";

let directory: string;
let record: string;
beforeEach(async () => {
  directory = await mkdtemp(join(tmpdir(), "agentsfleet-process-proof-"));
  record = join(directory, "process.json");
  vi.stubEnv("ACCEPTANCE_RUNNER_PROCESS_EVIDENCE", record);
});
afterEach(async () => {vi.unstubAllGlobals(); vi.unstubAllEnvs(); await rm(directory, {recursive: true, force: true});});

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

describe("protected dashboard build identity", () => {
  it("uses the bypass credential on the bounded header request", async () => {
    const secret = crypto.randomUUID();
    vi.stubEnv("VERCEL_BYPASS_SECRET", secret);
    const fetch = vi.fn().mockResolvedValue(new Response(null, {headers:{"x-agentsfleet-build":"candidate"}}));
    vi.stubGlobal("fetch", fetch);
    expect(await readAppBuild("https://dashboard.example.test")).toBe("candidate");
    expect(fetch).toHaveBeenCalledWith("https://dashboard.example.test", expect.objectContaining({
      method:"HEAD", redirect:"manual", headers:{"x-vercel-protection-bypass":secret},
    }));
  });
  it("can observe an unprotected dashboard without credentials", async () => {
    vi.stubEnv("VERCEL_BYPASS_SECRET", "");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(null)));
    expect(await readAppBuild("https://dashboard.example.test")).toBeNull();
  });
  for (const status of [401,403,404,500]) {
    it(`refuses an HTTP ${status} response even if it carries a build header`, async () => {
      vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(null, {status,headers:{"x-agentsfleet-build":"candidate"}})));
      await expect(readAppBuild("https://dashboard.example.test")).rejects.toThrow(`${status}`);
    });
  }
});
