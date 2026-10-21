// @vitest-environment node
import { expect, it, vi } from "vitest";
import { DraftWatch } from "@/lib/api/draft-watch";

const ID = "draft";

it("should preserve a replacement when the stopped owner finishes late", async () => {
  const first = Promise.withResolvers<void>();
  const second = Promise.withResolvers<void>();
  const view = new AbortController();
  const watcher = new DraftWatch();
  const follow = vi.fn<(signal: AbortSignal) => Promise<void>>().mockResolvedValue(undefined).mockImplementationOnce(() => first.promise)
    .mockImplementationOnce(() => second.promise);
  const failed = vi.fn();
  watcher.start(ID, view.signal, follow, failed);
  watcher.start(ID, view.signal, follow, failed);
  expect(follow).toHaveBeenCalledTimes(1);
  watcher.stop();
  expect(follow.mock.calls[0]?.[0].aborted).toBe(true);
  watcher.start(ID, view.signal, follow, failed);
  first.reject(new TypeError("late failure"));
  await vi.waitFor(() => expect(first.promise).rejects.toThrow("late failure"));
  watcher.start(ID, view.signal, follow, failed);
  expect(follow).toHaveBeenCalledTimes(2);
  expect(failed).not.toHaveBeenCalled();
  expect(follow.mock.calls[1]?.[0].aborted).toBe(false);
  second.resolve();
  await vi.waitFor(() => { watcher.start(ID, view.signal, follow, failed); expect(follow).toHaveBeenCalledTimes(3); });
  watcher.stop();
});

it("should report a failure when the current owner is still active", async () => {
  const watcher = new DraftWatch();
  const view = new AbortController();
  const cause = new TypeError("unavailable");
  const failed = vi.fn();
  watcher.start(ID, view.signal, async () => { throw cause; }, failed);
  await vi.waitFor(() => expect(failed).toHaveBeenCalledWith(cause));
  watcher.stop();
});
