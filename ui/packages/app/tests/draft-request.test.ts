// @vitest-environment node
import { afterEach, describe, expect, it, vi } from "vitest";
import { z } from "zod";
import { definiteRefusal, losesAccess, setupErrorMessage, setupRequest, SetupUnavailable } from "@/lib/api/draft-request";
import { response } from "./helpers/draft-fixtures";

const Shape = z.object({value: z.string()});
const OVERSIZED_RESPONSE_BYTES = 4 * 1024 * 1024 + 1;
const signal = (): AbortSignal => new AbortController().signal;
afterEach(() => vi.unstubAllGlobals());

describe("bounded private setup transport", () => {
  it("shows curated refusals but never exposes raw problem detail", async () => {
    expect.assertions(4);
    const fetch = vi.fn().mockResolvedValueOnce(response({detail: "private-internal-trace", user_message: "Choose a model first.", error_code: "model_required"}, 409))
      .mockResolvedValueOnce(response({detail: "private-internal-trace"}, 502));
    vi.stubGlobal("fetch", fetch);
    await expect(setupRequest({path: "/live/test"}, Shape, signal())).rejects.toMatchObject({message: "Choose a model first.", status: 409, code: "model_required"});
    try { await setupRequest({path: "/live/test"}, Shape, signal()); } catch (cause) {
      expect(setupErrorMessage(cause)).not.toContain("private-internal-trace");
      expect(setupErrorMessage(cause)).toContain("Refresh the page");
    }
    expect(setupErrorMessage(new Error("private-internal-trace"))).not.toContain("private-internal-trace");
  });
  it("uses same-origin cookies and preserves conditional mutation headers", async () => {
    const fetch = vi.fn().mockResolvedValue(response({value: "ok"})); vi.stubGlobal("fetch", fetch);
    await expect(setupRequest({path: "/live/test", method: "POST", body: {text: "private"}, headers: {"If-Match": '"one"'}}, Shape, signal())).resolves.toEqual({value: "ok"});
    expect(fetch).toHaveBeenCalledWith("/live/test", expect.objectContaining({credentials: "same-origin", cache: "no-store", redirect: "error", body: '{"text":"private"}',
      headers: expect.objectContaining({"If-Match": '"one"'})}));
    expect(fetch.mock.calls[0]?.[1].headers).not.toHaveProperty("Authorization");
  });
  it.each([401, 403, 404, 410])("preserves access refusal %i even for an invalid body", async (status) => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("not-json", {status})));
    await expect(setupRequest({path: "/live/test"}, Shape, signal())).rejects.toMatchObject({status});
    expect(losesAccess(new SetupUnavailable("refused", status))).toBe(true);
  });
  it("preserves the fresh Review digest and resource tag on refusal", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response({detail: "Review changed", error_code: "review_required", review_digest: "fresh"}, 409, {ETag: '"two"'})));
    await expect(setupRequest({path: "/live/test"}, Shape, signal())).rejects.toMatchObject({status: 409, code: "review_required", reviewDigest: "fresh", etag: '"two"'});
  });
  it("treats network failure and malformed successful replies as uncertain", async () => {
    const fetch = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce(response({wrong: true})); vi.stubGlobal("fetch", fetch);
    for (let attempt = 0; attempt < 2; attempt += 1) await expect(setupRequest({path: "/live/test"}, Shape, signal())).rejects.toMatchObject({status: 502});
    expect(definiteRefusal(new SetupUnavailable("unknown", 408))).toBe(false);
    expect(losesAccess(new Error("offline"))).toBe(false);
  });
  it("cancels an oversized response without buffering its remaining body", async () => {
    const cancelled = vi.fn();
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(new ReadableStream({start(controller) {controller.enqueue(new Uint8Array(OVERSIZED_RESPONSE_BYTES));}, cancel: cancelled}))));
    await expect(setupRequest({path: "/live/test"}, Shape, signal())).rejects.toMatchObject({status: 502});
    expect(cancelled).toHaveBeenCalledTimes(1);
  });
  it("does not start a request after disposal", async () => {
    const fetch = vi.fn(); vi.stubGlobal("fetch", fetch);
    const controller = new AbortController(); controller.abort();
    await expect(setupRequest({path: "/live/test"}, Shape, controller.signal)).rejects.toMatchObject({status: 502});
    expect(fetch).not.toHaveBeenCalled();
  });
  it("releases a body when its owner aborts before fetch returns", async () => {
    const owner = new AbortController();
    const cancel = vi.fn();
    const stream = new ReadableStream({cancel});
    vi.stubGlobal("fetch", vi.fn(async () => {owner.abort(); return new Response(stream);}));
    await expect(setupRequest({path: "/live/test"}, Shape, owner.signal)).rejects.toMatchObject({status: 502});
    expect(cancel).toHaveBeenCalledOnce();
    expect(stream.locked).toBe(false);
  });
  it("accepts an empty successful delete only when its parser permits null", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response(null, 204)));
    await expect(setupRequest({path: "/live/test", method: "DELETE"}, z.null(), signal())).resolves.toBeNull();
  });
  it("refuses an empty response to a read", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(null)));
    await expect(setupRequest({path: "/live/test"}, Shape, signal())).rejects.toMatchObject({status: 502, message: "The setup response was empty"});
  });
  it.each([false, true])("cancels an interrupted response even when cancellation rejects=%s", async (refused) => {
    const started = Promise.withResolvers<void>();
    const cancel = vi.fn(() => refused ? Promise.reject(new Error("cancel refused")) : undefined);
    const stream = new ReadableStream<Uint8Array>({pull() {started.resolve();}, cancel});
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(stream)));
    const owner = new AbortController();
    const pending = setupRequest({path: "/live/test"}, Shape, owner.signal);
    await started.promise;
    // The reader begins after fetch resolves; let its abort handler attach.
    await Promise.resolve(); await Promise.resolve(); owner.abort();
    await expect(pending).rejects.toMatchObject({status: 502});
    expect(cancel).toHaveBeenCalledOnce(); expect(stream.locked).toBe(false);
  });
});
