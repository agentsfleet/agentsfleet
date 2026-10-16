// @vitest-environment node
import { afterEach, describe, expect, it, vi } from "vitest";
import { DraftOperation } from "@/lib/api/draft-operation";
import { SetupUnavailable } from "@/lib/api/draft-request";

afterEach(() => vi.restoreAllMocks());

describe("uncertain private setup operations", () => {
  it("does not restore an operation cleared while its acknowledgement was pending", async () => {
    const operation = new DraftOperation();
    const pending = Promise.withResolvers<void>();
    const running = operation.run({}, () => pending.promise);
    operation.clear(); pending.resolve(); await running;
    expect(operation.pending).toBe(false);
    await expect(operation.retry()).rejects.toMatchObject({status: 409});
  });
  it("reuses a pending operation for identical input and never resurrects a cleared refusal", async () => {
    const operation = new DraftOperation(); const keys: string[] = [];
    const execute = vi.fn(async (key: string) => { keys.push(key); if (keys.length === 1) throw new SetupUnavailable("unknown", 503); });
    await expect(operation.run({save: "one"}, execute)).rejects.toMatchObject({status: 503});
    await operation.run({save: "one"}, execute);
    expect(keys[0]).toBe(keys[1]); expect(operation.pending).toBe(false);
    const pending = Promise.withResolvers<void>();
    const running = operation.run({}, () => pending.promise);
    operation.clear(); pending.reject(new SetupUnavailable("refused", 409));
    await expect(running).rejects.toMatchObject({status: 409});
    expect(operation.pending).toBe(false);
  });
  it("reuses one identity and original body after an unconfirmed answer", async () => {
    const operation = new DraftOperation();
    const keys: string[] = [];
    const execute = vi.fn(async (key: string) => { keys.push(key); if (keys.length === 1) throw new SetupUnavailable("unknown"); });
    await expect(operation.run({save: "one"}, execute)).rejects.toMatchObject({status: 502});
    expect(operation.pending).toBe(true);
    await expect(operation.run({save: "two"}, execute)).rejects.toMatchObject({status: 409});
    expect(execute).toHaveBeenCalledTimes(1);
    await operation.retry();
    expect(keys[0]).toMatch(/^[0-9a-f-]{36}$/); expect(keys[1]).toBe(keys[0]); expect(operation.pending).toBe(false);
  });
  it.each([400, 401, 403, 404, 409, 410, 412, 429])("clears a definite refusal %i", async (status) => {
    const operation = new DraftOperation();
    await expect(operation.run({}, async () => { throw new SetupUnavailable("refused", status); })).rejects.toMatchObject({status});
    expect(operation.pending).toBe(false);
    await expect(operation.retry()).rejects.toMatchObject({status: 409});
  });
  it.each([408, 500, 502, 503])("retains an uncertain outcome %i", async (status) => {
    const operation = new DraftOperation();
    await expect(operation.run({}, async () => { throw new SetupUnavailable("unknown", status); })).rejects.toMatchObject({status});
    expect(operation.pending).toBe(true); operation.clear(); expect(operation.pending).toBe(false);
  });
  it("refuses simultaneous sends without replacing the original request", async () => {
    const operation = new DraftOperation();
    let release: (() => void) | undefined;
    const first = operation.run({}, () => new Promise<void>((resolve) => { release = resolve; }));
    await expect(operation.retry()).rejects.toMatchObject({status: 409});
    expect(operation.pending).toBe(true); release?.(); await first; expect(operation.pending).toBe(false);
  });
});
