// @vitest-environment node
import { expect, it, vi } from "vitest";
import { boundedBody, sameOrigin } from "@/lib/api/setup-proxy/body";

const URL = "https://app.example.test/private";
const request = (body: ReadableStream<Uint8Array>) => new Request(URL, {method: "POST", body, duplex: "half"} as RequestInit);

it("accepts empty input and uses the host when forwarded headers are absent", async () => {
  expect(await boundedBody(new Request(URL), new AbortController().signal)).toEqual(new Uint8Array());
  expect(sameOrigin(new Request(URL, {headers: {origin: "https://public.test", host: "public.test"}}))).toBe(true);
});

it.each([false, true])("cancels a blocked private body when its owner aborts, including cancel refusal=%s", async (refused) => {
  const started = Promise.withResolvers<void>();
  const cancel = vi.fn(() => {if (refused) return Promise.reject(new Error("cancel refused"));});
  const stream = new ReadableStream<Uint8Array>({pull: () => {started.resolve();}, cancel});
  const owner = new AbortController();
  const pending = boundedBody(request(stream), owner.signal);
  await started.promise; owner.abort();
  expect(await pending).toBeNull(); expect(cancel).toHaveBeenCalledOnce();
  expect(stream.locked).toBe(false);
});

it("releases a failed request stream and propagates the read error", async () => {
  const failure = new Error("reader failed");
  const stream = new ReadableStream<Uint8Array>({pull(controller) {controller.error(failure);}});
  await expect(boundedBody(request(stream), new AbortController().signal)).rejects.toBe(failure);
  expect(stream.locked).toBe(false);
});
