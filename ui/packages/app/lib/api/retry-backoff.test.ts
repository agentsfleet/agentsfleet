import { describe, expect, it, vi } from "vitest";
import { sleepUnlessAborted } from "./retry-backoff";

describe("sleepUnlessAborted", () => {
  it("a signal already aborted skips the sleep and settles at once", async () => {
    const sleep = vi.fn(() => new Promise<void>(() => {}));
    const controller = new AbortController();
    controller.abort();
    await sleepUnlessAborted(sleep, 60_000, controller.signal);
    expect(sleep).not.toHaveBeenCalled();
  });
});
