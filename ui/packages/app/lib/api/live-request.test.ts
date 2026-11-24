// @vitest-environment node
import { afterEach, describe, expect, it, vi } from "vitest";
import { z } from "zod";
import { UNCONFIRMED_REQUEST_MESSAGE } from "@/lib/errors";
import { definiteRefusal, liveRequest, losesAccess, RequestFailed, requestErrorMessage } from "./live-request";
import { response } from "@/tests/helpers/fleet-fixtures";

const Shape = z.object({value: z.string()});
const signal = (): AbortSignal => new AbortController().signal;
afterEach(() => vi.unstubAllGlobals());

describe("a browser request through the live proxy", () => {
  it("uses same-origin cookies, never a bearer, and sends a body and a version header as given", async () => {
    const fetch = vi.fn(async (_path: string, _init: RequestInit) => response({value: "ok"})); vi.stubGlobal("fetch", fetch);
    await expect(liveRequest({path: "/live/test", method: "POST", body: {text: "private"}, headers: {"If-Match": '"one"'}}, Shape, signal())).resolves.toEqual({value: "ok"});
    expect(fetch).toHaveBeenCalledWith("/live/test", expect.objectContaining({credentials: "same-origin", cache: "no-store", redirect: "error", body: '{"text":"private"}',
      headers: expect.objectContaining({"If-Match": '"one"', "Content-Type": "application/json"})}));
    expect(fetch.mock.calls[0]?.[1].headers).not.toHaveProperty("Authorization");
    await liveRequest({path: "/live/read"}, Shape, signal());
    const read = fetch.mock.calls[1]?.[1];
    expect(read).toMatchObject({method: "GET"});
    expect(read).not.toHaveProperty("body");
    expect(read?.headers).not.toHaveProperty("Content-Type");
  });
  it("shows a curated refusal with its code and version, never a raw problem detail", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(response({detail: "private-internal-trace", user_message: "Choose a model first.", error_code: "UZ-SETUP-008"}, 409, {ETag: '"two"'}))
      .mockResolvedValueOnce(response({detail: "private-internal-trace"}, 409)));
    await expect(liveRequest({path: "/live/test"}, Shape, signal())).rejects.toMatchObject({message: "Choose a model first.", status: 409, code: "UZ-SETUP-008", etag: '"two"'});
    const second = await liveRequest({path: "/live/test"}, Shape, signal()).catch((cause: unknown) => cause);
    expect(requestErrorMessage(second)).toContain("Refresh the page");
    expect(requestErrorMessage(second)).not.toContain("private-internal-trace");
  });
  it.each([401, 403, 404, 410])("keeps the access refusal %i even for a body that is not JSON", async (status) => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("not-json", {status})));
    const failure = await liveRequest({path: "/live/test"}, Shape, signal()).catch((cause: unknown) => cause);
    expect(failure).toMatchObject({status, code: undefined});
    expect(losesAccess(failure)).toBe(true);
    expect(definiteRefusal(failure)).toBe(true);
  });
  it("names each access refusal in its own words", () => {
    expect(requestErrorMessage(new RequestFailed("x", 401))).toContain("sign-in session has expired");
    expect(requestErrorMessage(new RequestFailed("x", 403))).toContain("doesn’t have access");
    expect(requestErrorMessage(new RequestFailed("x", 404))).toContain("no longer available");
    expect(requestErrorMessage(new RequestFailed("x", 410))).toContain("no longer available");
    expect(requestErrorMessage(new RequestFailed("The name is taken.", 409))).toBe("The name is taken.");
    expect(requestErrorMessage(new Error("private-internal-trace"))).toBe(UNCONFIRMED_REQUEST_MESSAGE);
  });
  it("treats a lost answer and a malformed success as unconfirmed, and a timeout as no refusal", async () => {
    const fetch = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce(response({wrong: true})); vi.stubGlobal("fetch", fetch);
    for (let attempt = 0; attempt < 2; attempt += 1) await expect(liveRequest({path: "/live/test"}, Shape, signal())).rejects.toMatchObject({status: 502});
    expect(definiteRefusal(new RequestFailed("timeout", 408))).toBe(false);
    expect(definiteRefusal(new RequestFailed("unconfirmed", 502))).toBe(false);
    expect(definiteRefusal(new Error("offline"))).toBe(false);
    expect(losesAccess(new Error("offline"))).toBe(false);
    expect(losesAccess(new RequestFailed("conflict", 409))).toBe(false);
  });
  it("starts no request after its owner aborted", async () => {
    const fetch = vi.fn(); vi.stubGlobal("fetch", fetch);
    const controller = new AbortController(); controller.abort();
    await expect(liveRequest({path: "/live/test"}, Shape, controller.signal)).rejects.toMatchObject({status: 502});
    expect(fetch).not.toHaveBeenCalled();
  });
  it("releases a body when its owner aborts before fetch returns", async () => {
    const owner = new AbortController();
    const cancel = vi.fn();
    const stream = new ReadableStream({cancel});
    vi.stubGlobal("fetch", vi.fn(async () => {owner.abort(); return new Response(stream);}));
    await expect(liveRequest({path: "/live/test"}, Shape, owner.signal)).rejects.toMatchObject({status: 502});
    expect(cancel).toHaveBeenCalledOnce();
    expect(stream.locked).toBe(false);
  });
  it("accepts an empty delete only when its parser permits null, and refuses an empty read", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(response(null, 204)).mockResolvedValueOnce(new Response(null)));
    await expect(liveRequest({path: "/live/test", method: "DELETE"}, z.null(), signal())).resolves.toBeNull();
    await expect(liveRequest({path: "/live/test"}, Shape, signal())).rejects.toMatchObject({status: 502, message: "The response was empty"});
  });
  it("aborts after the body was read but before its answer is handed back", async () => {
    const owner = new AbortController();
    vi.stubGlobal("fetch", vi.fn(async () => new Response(new ReadableStream({start(controller) {
      controller.enqueue(new TextEncoder().encode('{"value":"late"}')); controller.close(); owner.abort();
    }}))));
    await expect(liveRequest({path: "/live/test"}, Shape, owner.signal)).rejects.toMatchObject({status: 502});
  });
  it.each([false, true])("cancels an interrupted response even when cancellation rejects=%s", async (refused) => {
    const started = Promise.withResolvers<void>();
    const cancel = vi.fn(() => refused ? Promise.reject(new Error("cancel refused")) : undefined);
    const stream = new ReadableStream<Uint8Array>({pull() {started.resolve();}, cancel});
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(stream)));
    const owner = new AbortController();
    const pending = liveRequest({path: "/live/test"}, Shape, owner.signal);
    await started.promise;
    // The reader begins after fetch resolves; let its abort handler attach.
    await Promise.resolve(); await Promise.resolve(); owner.abort();
    await expect(pending).rejects.toMatchObject({status: 502});
    expect(cancel).toHaveBeenCalledOnce(); expect(stream.locked).toBe(false);
  });
});
