import { MessageNotSentError, useExternalStoreRuntime, type AppendMessage, type AssistantRuntime, type ThreadMessageLike } from "@assistant-ui/react";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useEffect, useRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { PENDING_SEND_STATE, useFleetPendingSends } from "./useFleetPendingSends";
import { SEND_TIMEOUT_MS, useMessageDelivery, useOpenFleet } from "./useFleetMessageDelivery";
import { useFleetUploadPreviews } from "../composer/useFleetUploadPreviews";
import { __resetPendingSendsForTests } from "@/lib/streaming/pending-sends";
import { sendMarks } from "@/lib/streaming/send-marks";
import { UUID_V7 } from "@/tests/fleet-thread/steer-copy";

// A chat message that opens with `@name`, through the real ledger, the real
// lookup and steer clients and a stub daemon at the network boundary. The open
// chat is fleet A's; `agentbob` is fleet B.

const router = vi.hoisted(() => ({ push: vi.fn() }));
vi.mock("next/navigation", () => ({ useRouter: () => router }));

const WORKSPACE = "0199a801-1000-7000-8000-000000000001";
const HERE = "0199a801-1000-7000-8000-0000000000a0";
const HERE_NAME = "support-triage";
const BOB = "0199a801-1000-7000-8000-0000000000b0";
const BOB_NAME = "agentbob";
const SUBJECT = "user_address";
const EVENT_ID = "0199a801-1000-7000-8000-0000000000e0";
const OPTIMISTIC_ROW = "optim-1";
const LIST_ROUTE = `/live/v1/workspaces/${WORKSPACE}/fleets?`;
const BOB_LOOKUP = `${LIST_ROUTE}name=agentbob`;
const AT = "2026-11-19T00:00:00Z";
const NO_EVENTS: never[] = [];
const NO_MESSAGES: ThreadMessageLike[] = [];

const network = vi.fn<(url: string, init?: RequestInit) => Promise<Response>>();

// The daemon behind the proxy: the fleet list answers `lookup`, and every steer is admitted.
function daemon(lookup: (signal: AbortSignal | undefined) => Response | Promise<Response>): void {
  network.mockImplementation(async (url, init) => {
    if (url.startsWith(LIST_ROUTE)) return lookup(init?.signal ?? undefined);
    if (url.endsWith("/messages") && init?.method === "POST") return Response.json({ status: "accepted", event_id: EVENT_ID, replayed: false }, { status: 202 });
    throw new Error(`Unexpected request: ${url}`);
  });
}

const summary = (id: string, name: string) => ({ id, name, description: null, status: "active", created_at: AT, updated_at: AT, events_processed: "0", budget_used_nanos: "0" });
const listOf = (...items: unknown[]) => Response.json({ items, total: null, next_cursor: null });
const messagesOf = (fleetId: string) => `/live/v1/workspaces/${WORKSPACE}/fleets/${fleetId}/messages`;
const lookups = () => network.mock.calls.flatMap(([url]) => (url.startsWith(LIST_ROUTE) ? [url] : []));
const posts = () => network.mock.calls.flatMap(([url, init]) =>
  (init?.method === "POST" && typeof init.body === "string" ? [{ url, body: JSON.parse(init.body) as unknown }] : []));

function message(text: string): AppendMessage {
  return { role: "user", content: [{ type: "text", text }], createdAt: new Date(0), metadata: { custom: {} }, parentId: null, sourceId: null, runConfig: undefined };
}

// One chat's delivery over the real ledger, with the thread's paint recorded.
function useChat(fleetId: string, fleetName: string, openFleet: (fleetId: string) => void, stream: ReturnType<typeof paint>) {
  const ledger = useFleetPendingSends({ subject: SUBJECT, workspaceId: WORKSPACE, fleetId });
  const delivery = useMessageDelivery({ workspaceId: WORKSPACE, fleetId, fleetName, openFleet, ...stream, writers: ledger.writers });
  return { ledger, delivery };
}

function paint() {
  return { appendOptimistic: vi.fn((_text: string, _actor: string) => OPTIMISTIC_ROW), reconcileOptimistic: vi.fn(), discardOptimistic: vi.fn() };
}

function chat(fleetId = HERE, fleetName = HERE_NAME) {
  const openFleet = vi.fn();
  const stream = paint();
  const hook = renderHook(() => useChat(fleetId, fleetName, openFleet, stream));
  return { hook, stream, openFleet };
}

// The thread's own composer over that delivery and the upload previews, wired as FleetThread wires them.
function composer(): AssistantRuntime["thread"]["composer"] {
  const { result } = renderHook(() => {
    const runtime = useRef<AssistantRuntime | null>(null);
    const { delivery } = useChat(HERE, HERE_NAME, vi.fn(), paint());
    const previews = useFleetUploadPreviews({ workspaceId: WORKSPACE, fleetId: HERE, subject: SUBJECT, events: NO_EVENTS, onNew: delivery.onNew, runtime });
    const thread = useExternalStoreRuntime({ messages: NO_MESSAGES, convertMessage: (entry: ThreadMessageLike) => entry,
      onNew: previews.onNew, adapters: { attachments: previews.attachments } });
    useEffect(() => { runtime.current = thread; }, [thread]);
    return thread;
  });
  return result.current.thread.composer;
}

async function refused(hook: ReturnType<typeof chat>["hook"], text: string): Promise<void> {
  await act(async () => {
    await expect(hook.result.current.delivery.onNew(message(text))).rejects.toBeInstanceOf(MessageNotSentError);
  });
}

const ended = (hook: ReturnType<typeof chat>["hook"]) => hook.result.current.ledger.writers.list().map(({ text, state }) => ({ text, state }));

beforeEach(() => {
  vi.stubGlobal("fetch", network);
});

afterEach(() => {
  cleanup();
  network.mockReset();
  router.push.mockReset();
  vi.unstubAllGlobals();
  vi.useRealTimers();
  sendMarks.reset();
  __resetPendingSendsForTests();
});

describe("an addressed chat message", () => {
  it("delivery_sends_an_addressed_message_to_the_named_fleet", async () => {
    daemon(() => listOf(summary(BOB, BOB_NAME)));
    const { hook, stream, openFleet } = chat();
    const begin = vi.spyOn(hook.result.current.ledger.writers, "begin");
    await act(() => hook.result.current.delivery.onNew(message("@agentbob retry")));
    const sent = begin.mock.calls[0]?.[0];
    // The ledger and the optimistic row keep the words as typed.
    expect(sent?.text).toBe("@agentbob retry");
    expect(stream.appendOptimistic).toHaveBeenCalledWith("@agentbob retry", expect.any(String), undefined);
    expect(lookups()).toEqual([BOB_LOOKUP]);
    expect(sent?.operationId).toMatch(UUID_V7);
    expect(posts()).toEqual([{ url: messagesOf(BOB), body: { message: "retry", operation_id: sent?.operationId } }]);
    // Settled, and shown in B's chat rather than this one.
    expect(hook.result.current.ledger.writers.list()).toEqual([]);
    expect(stream.discardOptimistic).toHaveBeenCalledExactlyOnceWith(OPTIMISTIC_ROW);
    expect(stream.reconcileOptimistic).not.toHaveBeenCalled();
    expect(openFleet).toHaveBeenCalledExactlyOnceWith(BOB);
  });

  it("delivery_refuses_an_unknown_or_empty_address", async () => {
    daemon(() => listOf());
    const { hook, stream, openFleet } = chat();
    await refused(hook, "@nobody retry");
    expect(lookups()).toEqual([`${LIST_ROUTE}name=nobody`]);
    // An address with no words after it makes no lookup.
    await refused(hook, "@agentbob");
    expect(lookups()).toHaveLength(1);
    expect(ended(hook)).toEqual([
      { text: "@nobody retry", state: PENDING_SEND_STATE.REFUSED },
      { text: "@agentbob", state: PENDING_SEND_STATE.REFUSED },
    ]);
    expect(stream.discardOptimistic).toHaveBeenCalledTimes(2);
    // An upload belongs to the open fleet: the file is refused before it leaves, and the draft comes back with it.
    const draft = composer();
    await act(() => draft.addAttachment(new File(["notes"], "notes.txt", { type: "text/plain" })));
    act(() => draft.setText("@agentbob look"));
    const failure = vi.spyOn(console, "error").mockImplementation(() => undefined);
    act(() => draft.send());
    await waitFor(() => expect(draft.getState().text).toBe("@agentbob look"));
    failure.mockRestore();
    expect(draft.getState().attachments).toEqual([expect.objectContaining({ name: "notes.txt", status: expect.objectContaining({ type: "incomplete" }) })]);
    expect(lookups()).toHaveLength(1);
    expect(network.mock.calls.filter(([url]) => url.endsWith("/uploads"))).toEqual([]);
    expect(posts()).toEqual([]);
    expect(openFleet).not.toHaveBeenCalled();
  });

  it("delivery_sends_a_self_address_here", async () => {
    daemon(() => listOf(summary(BOB, BOB_NAME)));
    const { hook, stream, openFleet } = chat(BOB, BOB_NAME);
    await act(() => hook.result.current.delivery.onNew(message("@agentbob retry")));
    expect(lookups()).toEqual([]);
    expect(posts()).toEqual([{ url: messagesOf(BOB), body: { message: "retry", operation_id: expect.stringMatching(UUID_V7) } }]);
    expect(stream.reconcileOptimistic).toHaveBeenCalledExactlyOnceWith(OPTIMISTIC_ROW, EVENT_ID, false, null);
    expect(stream.discardOptimistic).not.toHaveBeenCalled();
    expect(openFleet).not.toHaveBeenCalled();
  });

  it("delivery_maps_a_failed_lookup_like_a_failed_send", async () => {
    const { hook } = chat();
    daemon(() => Response.json({ detail: "Not authenticated" }, { status: 401 }));
    await refused(hook, "@agentbob retry");
    daemon(() => Response.json({ detail: "Unavailable" }, { status: 503 }));
    await refused(hook, "@agentbob rerun");
    vi.useFakeTimers();
    // A lookup that outlasts the send's deadline is aborted with it.
    daemon((signal) => new Promise((_resolve, reject) => {
      signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")), { once: true });
    }));
    const late = hook.result.current.delivery.onNew(message("@agentbob later")).catch((error: unknown) => error);
    await vi.advanceTimersByTimeAsync(SEND_TIMEOUT_MS - 1);
    expect(ended(hook).at(-1)).toEqual({ text: "@agentbob later", state: PENDING_SEND_STATE.SENDING });
    await vi.advanceTimersByTimeAsync(1);
    expect(await late).toBeInstanceOf(MessageNotSentError);
    expect(ended(hook)).toEqual([
      { text: "@agentbob retry", state: PENDING_SEND_STATE.SESSION },
      { text: "@agentbob rerun", state: PENDING_SEND_STATE.UNKNOWN },
      { text: "@agentbob later", state: PENDING_SEND_STATE.UNKNOWN },
    ]);
    expect(lookups()).toEqual([BOB_LOOKUP, BOB_LOOKUP, BOB_LOOKUP]);
    expect(posts()).toEqual([]);
  });

  it("delivery_resends_an_addressed_message_to_the_named_fleet", async () => {
    const saved: unknown[] = [];
    daemon(() => listOf(...saved));
    const { hook, openFleet } = chat();
    await refused(hook, "@agentbob retry");
    const [entry] = hook.result.current.ledger.writers.list();
    if (entry === undefined) throw new Error("the refused send left no ledger entry");
    expect(entry).toMatchObject({ text: "@agentbob retry", state: PENDING_SEND_STATE.REFUSED });
    saved.push(summary(BOB, BOB_NAME));
    act(() => hook.result.current.delivery.resend(entry.operationId));
    await waitFor(() => expect(openFleet).toHaveBeenCalledExactlyOnceWith(BOB));
    expect(lookups()).toEqual([BOB_LOOKUP, BOB_LOOKUP]);
    expect(posts()).toEqual([{ url: messagesOf(BOB), body: { message: "retry", operation_id: entry.operationId } }]);
    expect(hook.result.current.ledger.writers.list()).toEqual([]);
  });

  it("delivery_resends_to_the_fleet_the_first_attempt_resolved_even_if_the_name_moved", async () => {
    const OTHER = "0199a801-1000-7000-8000-0000000000c0";
    let saved = summary(BOB, BOB_NAME);
    network.mockImplementation(async (url, init) => {
      if (url.startsWith(LIST_ROUTE)) return listOf(saved);
      if (url.endsWith("/messages") && init?.method === "POST") return Response.json({ detail: "Unavailable" }, { status: 503 });
      throw new Error(`Unexpected request: ${url}`);
    });
    const { hook, openFleet } = chat();
    await refused(hook, "@agentbob retry");
    const [entry] = hook.result.current.ledger.writers.list();
    if (entry === undefined) throw new Error("the unconfirmed send left no ledger entry");
    expect(entry.state).toBe(PENDING_SEND_STATE.UNKNOWN);
    // The name now belongs to another fleet, and the daemon takes the retry.
    saved = summary(OTHER, BOB_NAME);
    daemon(() => listOf(saved));
    act(() => hook.result.current.delivery.resend(entry.operationId));
    await waitFor(() => expect(openFleet).toHaveBeenCalledExactlyOnceWith(BOB));
    expect(lookups()).toEqual([BOB_LOOKUP]);
    expect(posts().map(({ url }) => url)).toEqual([messagesOf(BOB), messagesOf(BOB)]);
    expect(hook.result.current.ledger.writers.list()).toEqual([]);
  });

  it("opens a fleet's chat at that fleet's page in the workspace", () => {
    const { result } = renderHook(() => useOpenFleet(WORKSPACE));
    result.current(BOB);
    expect(router.push).toHaveBeenCalledExactlyOnceWith(`/w/${WORKSPACE}/fleets/${BOB}`);
  });
});
