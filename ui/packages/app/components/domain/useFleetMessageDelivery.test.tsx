import { MessageNotSentError, type AppendMessage } from "@assistant-ui/react";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { PENDING_SEND_STATE, type PendingSend, type PendingSendWriters } from "./useFleetPendingSends";
import { DEFAULT_REQUEST_TIMEOUT_MS } from "@/lib/api/client";
import { RETRY_DEFAULTS } from "@/lib/api/retry-config";
import { ERROR_CODE } from "@/lib/errors";

const postSteer = vi.hoisted(() => vi.fn());
const mint = vi.hoisted(() => ({ unavailable: false }));
vi.mock("@/lib/api/fleet-steer", () => ({ postSteer }));
vi.mock("@/lib/onboarding-refresh", () => ({ requestOnboardingRefresh: vi.fn() }));
vi.mock("@/lib/streaming/operation-id", async (importActual) => {
  const actual = await importActual<typeof import("@/lib/streaming/operation-id")>();
  return {
    ...actual,
    mintOperationId: () => {
      if (mint.unavailable) throw new actual.MintUnavailable();
      return actual.mintOperationId();
    },
  };
});

const { SEND_TIMEOUT_MS, useMessageDelivery } = await import("./useFleetMessageDelivery");

const WORKSPACE = "ws_delivery";
const FLEET = "fleet_delivery";
const EVENT_ID = "1790573387481-566";
const NO_ROW = "";
const OPTIMISTIC_ROW = "optim-1";
const DEPLOY = "deploy";
const STOP = "stop";
const ACCEPTED = { ok: true, data: { status: "accepted", event_id: EVENT_ID, replayed: false } };
const REPLAYED = { ok: true, data: { status: "accepted", event_id: EVENT_ID, replayed: true } };
const OPERATION_CONFLICT = { ok: false, error: "conflict", status: 409, errorCode: ERROR_CODE.AGENTSFLEET_OPERATION_CONFLICT };
// Ten seconds into the first send's thirty.
const LATER_MS = 10_000;
const never = () => new Promise<never>(() => undefined);

function message(text: string): AppendMessage {
  return {
    role: "user",
    content: [{ type: "text", text }],
    createdAt: new Date(0),
    metadata: { custom: {} },
    parentId: null,
    sourceId: null,
    runConfig: undefined,
  };
}

function delivery(appendOptimistic: (text: string, actor: string) => string = () => OPTIMISTIC_ROW) {
  const writers: PendingSendWriters = {
    begin: vi.fn(),
    settle: vi.fn(),
    fail: vi.fn(),
    dismiss: vi.fn(),
    find: vi.fn(() => undefined),
    list: vi.fn(() => []),
  };
  const ctx = {
    workspaceId: WORKSPACE,
    fleetId: FLEET,
    appendOptimistic,
    reconcileOptimistic: vi.fn(),
    discardOptimistic: vi.fn(),
    onSubmitted: vi.fn(),
    writers,
  };
  return { ctx, hook: renderHook(() => useMessageDelivery(ctx)) };
}

// The operation id and the signal of the `call`th steer, counted from one.
function sentId(call: number): string {
  return postSteer.mock.calls[call - 1]?.[3] as string;
}

function signalOf(call: number): AbortSignal {
  return postSteer.mock.calls[call - 1]?.[4] as AbortSignal;
}

// A ledger the delivery reads back: every entry it ended, in the state it
// ended, and gone from `find` once dismissed, as the real ledger's tombstone is.
function ledgerOf(writers: PendingSendWriters): void {
  const ended = new Map<string, PendingSend>();
  vi.mocked(writers.fail).mockImplementation((operationId, state) => {
    ended.set(operationId, { operationId, text: DEPLOY, state, submittedAtMs: 0 });
  });
  vi.mocked(writers.dismiss).mockImplementation((operationId) => {
    ended.set(operationId, { operationId, text: "", state: PENDING_SEND_STATE.DISMISSED, submittedAtMs: 0 });
  });
  vi.mocked(writers.find).mockImplementation((operationId) => {
    const entry = ended.get(operationId);
    return entry?.state === PENDING_SEND_STATE.DISMISSED ? undefined : entry;
  });
  vi.mocked(writers.list).mockImplementation(() => [...ended.values()]);
}

// The second steer went out, with its text, under an id of its own.
function expectSentAgainAsNew(text: string): void {
  expect(postSteer).toHaveBeenCalledTimes(2);
  expect(postSteer.mock.calls[1]?.[2]).toBe(text);
  expect(sentId(2)).toEqual(expect.any(String));
  expect(sentId(2)).not.toBe(sentId(1));
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.useRealTimers();
  mint.unavailable = false;
});

describe("useMessageDelivery", () => {
  it("still sends and settles when the fleet's thread is gone, and scrolls to no row", async () => {
    postSteer.mockResolvedValue(ACCEPTED);
    // The registry answers an empty id when the fleet's entry was released.
    const { ctx, hook } = delivery(() => NO_ROW);
    await act(() => hook.result.current.onNew(message(DEPLOY)));
    expect(postSteer).toHaveBeenCalledWith(WORKSPACE, FLEET, DEPLOY, expect.any(String), expect.any(AbortSignal));
    expect(ctx.writers.settle).toHaveBeenCalledTimes(1);
    expect(ctx.onSubmitted).not.toHaveBeenCalled();
  });

  it("hands a replayed 202 to the reconcile as a replay", async () => {
    postSteer.mockResolvedValue(REPLAYED);
    const { ctx, hook } = delivery();
    await act(() => hook.result.current.onNew(message(DEPLOY)));
    expect(ctx.reconcileOptimistic).toHaveBeenCalledExactlyOnceWith(OPTIMISTIC_ROW, EVENT_ID, true);
  });

  it("ignores a Resend for a send the ledger no longer holds", () => {
    // Another tab dismissed it between this render and the click.
    const { ctx, hook } = delivery();
    act(() => hook.result.current.resend("op-dismissed-elsewhere"));
    expect(ctx.writers.find).toHaveBeenCalledWith("op-dismissed-elsewhere");
    expect(ctx.writers.begin).not.toHaveBeenCalled();
    expect(postSteer).not.toHaveBeenCalled();
  });

  it("gives a send longer than the steer route's worst case: its deadline and one attempt", () => {
    expect(SEND_TIMEOUT_MS).toBeGreaterThanOrEqual(RETRY_DEFAULTS.deadlineMs + DEFAULT_REQUEST_TIMEOUT_MS);
  });
});

describe("a send that never answers", () => {
  it("test_hung_send_times_out_to_unknown", async () => {
    vi.useFakeTimers();
    postSteer.mockReturnValueOnce(never());
    const { ctx, hook } = delivery();
    const sent = hook.result.current.onNew(message(DEPLOY)).catch((error: unknown) => error);
    await vi.advanceTimersByTimeAsync(SEND_TIMEOUT_MS - 1);
    expect(ctx.writers.fail).not.toHaveBeenCalled();
    expect(signalOf(1).aborted).toBe(false);

    await vi.advanceTimersByTimeAsync(1);
    expect(await sent).toBeInstanceOf(MessageNotSentError);
    // Aborted on the wire, so nothing answers later: the entry stays unknown.
    expect(signalOf(1).aborted).toBe(true);
    expect(ctx.writers.fail).toHaveBeenCalledExactlyOnceWith(sentId(1), PENDING_SEND_STATE.UNKNOWN);
    expect(ctx.discardOptimistic).toHaveBeenCalledWith(OPTIMISTIC_ROW);
    expect(ctx.writers.settle).not.toHaveBeenCalled();
  });

  it("leaves the entry unknown when the transport throws instead of answering", async () => {
    postSteer.mockRejectedValueOnce(new Error("socket closed"));
    const { ctx, hook } = delivery();
    await expect(hook.result.current.onNew(message(DEPLOY))).rejects.toBeInstanceOf(MessageNotSentError);
    expect(ctx.writers.fail).toHaveBeenCalledExactlyOnceWith(sentId(1), PENDING_SEND_STATE.UNKNOWN);
  });

  it("test_queue_survives_a_hung_send", async () => {
    vi.useFakeTimers();
    postSteer.mockReturnValueOnce(never()).mockResolvedValueOnce(ACCEPTED);
    const { ctx, hook } = delivery();
    void hook.result.current.onNew(message(DEPLOY)).catch(() => undefined);
    await vi.advanceTimersByTimeAsync(LATER_MS);
    const second = hook.result.current.onNew(message(STOP));
    await vi.advanceTimersByTimeAsync(SEND_TIMEOUT_MS - LATER_MS - 1);
    expect(postSteer).toHaveBeenCalledTimes(1);

    // The hung send's clock frees the queue with ten seconds left on this one.
    await vi.advanceTimersByTimeAsync(1);
    await second;
    expect(postSteer).toHaveBeenLastCalledWith(WORKSPACE, FLEET, STOP, expect.any(String), expect.any(AbortSignal));
    expect(signalOf(2).aborted).toBe(false);
    expect(ctx.writers.settle).toHaveBeenCalledExactlyOnceWith(sentId(2));
  });

  it("ends a queued send when its own clock runs out, not a full clock after its turn", async () => {
    vi.useFakeTimers();
    postSteer.mockReturnValueOnce(never()).mockReturnValueOnce(never());
    const { ctx, hook } = delivery();
    void hook.result.current.onNew(message(DEPLOY)).catch(() => undefined);
    await vi.advanceTimersByTimeAsync(LATER_MS);
    const second = hook.result.current.onNew(message(STOP)).catch((error: unknown) => error);
    // Dispatched when the first send's time ran out, with ten seconds left.
    await vi.advanceTimersByTimeAsync(SEND_TIMEOUT_MS - LATER_MS);
    expect(postSteer).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(LATER_MS);
    expect(await second).toBeInstanceOf(MessageNotSentError);
    expect(signalOf(2).aborted).toBe(true);
    expect(ctx.writers.fail).toHaveBeenLastCalledWith(sentId(2), PENDING_SEND_STATE.UNKNOWN);
  });

  it("test_send_deadline_runs_from_send: ends a send whose clock ran out before its turn as not sent, without sending it", async () => {
    vi.useFakeTimers();
    // Both sent in one instant: their clocks run out together.
    postSteer.mockReturnValueOnce(never());
    const { ctx, hook } = delivery();
    void hook.result.current.onNew(message(DEPLOY)).catch(() => undefined);
    const queued = hook.result.current.onNew(message(STOP)).catch((error: unknown) => error);
    await vi.advanceTimersByTimeAsync(SEND_TIMEOUT_MS);
    // Its draft comes back, and its entry reads not sent: it never left.
    expect(await queued).toBeInstanceOf(MessageNotSentError);
    const queuedId = vi.mocked(ctx.writers.begin).mock.calls[1]?.[0].operationId;
    expect(queuedId).toEqual(expect.any(String));
    expect(ctx.writers.fail).toHaveBeenCalledWith(queuedId, PENDING_SEND_STATE.REFUSED);
    expect(ctx.writers.fail).toHaveBeenCalledWith(sentId(1), PENDING_SEND_STATE.UNKNOWN);
    expect(ctx.writers.fail).toHaveBeenCalledTimes(2);
    // Its turn, when it comes, sends nothing.
    await vi.advanceTimersByTimeAsync(0);
    expect(postSteer).toHaveBeenCalledTimes(1);
  });
});

describe("a reused operation id", () => {
  it("test_operation_conflict_offers_no_resend", async () => {
    postSteer.mockResolvedValue(OPERATION_CONFLICT);
    const { ctx, hook } = delivery();
    await expect(hook.result.current.onNew(message(DEPLOY))).rejects.toBeInstanceOf(MessageNotSentError);
    expect(ctx.writers.fail).toHaveBeenCalledWith(sentId(1), PENDING_SEND_STATE.CONFLICT);
  });

  it("test_conflict_draft_mints_a_new_id", async () => {
    postSteer.mockResolvedValueOnce(OPERATION_CONFLICT).mockResolvedValueOnce(ACCEPTED);
    const { ctx, hook } = delivery();
    ledgerOf(ctx.writers);
    await expect(hook.result.current.onNew(message(DEPLOY))).rejects.toBeInstanceOf(MessageNotSentError);
    // The same draft, restored and sent unchanged: a new message, not a replay.
    act(() => hook.result.current.noteRestored(sentId(1), DEPLOY));
    await act(() => hook.result.current.onNew(message(DEPLOY)));
    expectSentAgainAsNew(DEPLOY);
  });

  it("sends a conflict's text as new from the notice and dismisses the spent id", async () => {
    postSteer.mockResolvedValueOnce(OPERATION_CONFLICT).mockResolvedValueOnce(ACCEPTED);
    const { ctx, hook } = delivery();
    ledgerOf(ctx.writers);
    await expect(hook.result.current.onNew(message(DEPLOY))).rejects.toBeInstanceOf(MessageNotSentError);
    await act(async () => hook.result.current.resend(sentId(1)));
    expect(ctx.writers.dismiss).toHaveBeenCalledWith(sentId(1));
    expectSentAgainAsNew(DEPLOY);
  });

  it("test_composer_send_retires_its_conflict: dismisses a conflict whose text the composer sends, so its Send as new cannot send it twice", async () => {
    postSteer.mockResolvedValueOnce(OPERATION_CONFLICT).mockResolvedValue(ACCEPTED);
    const { ctx, hook } = delivery();
    ledgerOf(ctx.writers);
    await expect(hook.result.current.onNew(message(DEPLOY))).rejects.toBeInstanceOf(MessageNotSentError);
    // Typed again rather than restored: a new id, and the conflict is spent.
    await act(() => hook.result.current.onNew(message(DEPLOY)));
    expect(ctx.writers.dismiss).toHaveBeenCalledExactlyOnceWith(sentId(1));
    expectSentAgainAsNew(DEPLOY);
    // The notice's button, pressed late, finds nothing to send.
    await act(async () => hook.result.current.resend(sentId(1)));
    expect(postSteer).toHaveBeenCalledTimes(2);
  });

  it("sends nothing, and dismisses nothing, when no new id can be minted", async () => {
    postSteer.mockResolvedValueOnce(OPERATION_CONFLICT);
    const { ctx, hook } = delivery();
    ledgerOf(ctx.writers);
    await expect(hook.result.current.onNew(message(DEPLOY))).rejects.toBeInstanceOf(MessageNotSentError);
    mint.unavailable = true;
    act(() => hook.result.current.resend(sentId(1)));
    await expect(hook.result.current.onNew(message(DEPLOY))).rejects.toBeInstanceOf(MessageNotSentError);
    expect(ctx.writers.dismiss).not.toHaveBeenCalled();
    expect(postSteer).toHaveBeenCalledTimes(1);
  });
});
