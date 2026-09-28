import { MessageNotSentError, type AppendMessage } from "@assistant-ui/react";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { PENDING_SEND_STATE, type PendingSend, type PendingSendWriters } from "./useFleetPendingSends";

const steerFleetAction = vi.hoisted(() => vi.fn());
const mint = vi.hoisted(() => ({ unavailable: false }));
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({ steerFleetAction }));
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
const ACCEPTED = { ok: true, data: { event_id: EVENT_ID } };
const OPERATION_CONFLICT = { ok: false, error: "conflict", status: 409, errorCode: "UZ-AGT-016" };
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

function delivery(appendOptimistic: (text: string, actor: string) => string) {
  const writers: PendingSendWriters = {
    begin: vi.fn(),
    settle: vi.fn(),
    fail: vi.fn(),
    dismiss: vi.fn(),
    find: vi.fn(() => undefined),
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

// The operation id of the `call`th Server Action, counted from one.
function sentId(call: number): string {
  return steerFleetAction.mock.calls[call - 1]?.[3] as string;
}

// A ledger the delivery reads back: every entry it ended, in the state it ended.
function ledgerOf(writers: PendingSendWriters): void {
  const ended = new Map<string, PendingSend>();
  vi.mocked(writers.fail).mockImplementation((operationId, state) => {
    ended.set(operationId, { operationId, text: "deploy", state, submittedAtMs: 0 });
  });
  vi.mocked(writers.find).mockImplementation((operationId) => ended.get(operationId));
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.useRealTimers();
  mint.unavailable = false;
});

describe("useMessageDelivery", () => {
  it("still sends and settles when the fleet's thread is gone, and scrolls to no row", async () => {
    steerFleetAction.mockResolvedValue({ ok: true, data: { event_id: EVENT_ID } });
    // The registry answers an empty id when the fleet's entry was released.
    const { ctx, hook } = delivery(() => NO_ROW);
    await act(() => hook.result.current.onNew(message("deploy")));
    expect(steerFleetAction).toHaveBeenCalledWith(WORKSPACE, FLEET, "deploy", expect.any(String));
    expect(ctx.writers.settle).toHaveBeenCalledTimes(1);
    expect(ctx.onSubmitted).not.toHaveBeenCalled();
  });

  it("ignores a Resend for a send the ledger no longer holds", () => {
    // Another tab dismissed it between this render and the click.
    const { ctx, hook } = delivery(() => "optim-1");
    act(() => hook.result.current.resend("op-dismissed-elsewhere"));
    expect(ctx.writers.find).toHaveBeenCalledWith("op-dismissed-elsewhere");
    expect(ctx.writers.begin).not.toHaveBeenCalled();
    expect(steerFleetAction).not.toHaveBeenCalled();
  });
});

describe("a send that never answers", () => {
  it("test_hung_send_times_out_to_unknown", async () => {
    vi.useFakeTimers();
    let answer: (result: typeof ACCEPTED) => void = () => undefined;
    steerFleetAction.mockReturnValueOnce(new Promise((resolve) => (answer = resolve)));
    const { ctx, hook } = delivery(() => "optim-1");
    const sent = hook.result.current.onNew(message("deploy")).catch((error: unknown) => error);
    await vi.advanceTimersByTimeAsync(SEND_TIMEOUT_MS - 1);
    expect(ctx.writers.fail).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(await sent).toBeInstanceOf(MessageNotSentError);
    expect(ctx.writers.fail).toHaveBeenCalledWith(sentId(1), PENDING_SEND_STATE.UNKNOWN);
    expect(ctx.discardOptimistic).toHaveBeenCalledWith("optim-1");

    // The late answer settles the entry and paints nothing: its row is gone.
    answer(ACCEPTED);
    await vi.advanceTimersByTimeAsync(0);
    expect(ctx.writers.settle).toHaveBeenCalledWith(sentId(1));
    expect(ctx.reconcileOptimistic).not.toHaveBeenCalled();
  });

  it("leaves the entry unknown when the late answer is a refusal or a failure", async () => {
    vi.useFakeTimers();
    let refuse: (result: typeof OPERATION_CONFLICT) => void = () => undefined;
    let fail: (error: Error) => void = () => undefined;
    steerFleetAction
      .mockReturnValueOnce(new Promise((resolve) => (refuse = resolve)))
      .mockReturnValueOnce(new Promise((_, reject) => (fail = reject)));
    const { ctx, hook } = delivery(() => "optim-1");
    void hook.result.current.onNew(message("a")).catch(() => undefined);
    void hook.result.current.onNew(message("b")).catch(() => undefined);
    await vi.advanceTimersByTimeAsync(SEND_TIMEOUT_MS * 2);
    refuse(OPERATION_CONFLICT);
    fail(new Error("socket closed"));
    await vi.advanceTimersByTimeAsync(0);
    expect(ctx.writers.fail).toHaveBeenCalledTimes(2);
    expect(ctx.writers.settle).not.toHaveBeenCalled();
  });

  it("test_queue_survives_a_hung_send", async () => {
    vi.useFakeTimers();
    steerFleetAction.mockReturnValueOnce(never()).mockResolvedValueOnce(ACCEPTED);
    const { ctx, hook } = delivery(() => "optim-1");
    void hook.result.current.onNew(message("deploy")).catch(() => undefined);
    const second = hook.result.current.onNew(message("stop"));
    await vi.advanceTimersByTimeAsync(SEND_TIMEOUT_MS - 1);
    expect(steerFleetAction).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(1);
    await second;
    expect(steerFleetAction).toHaveBeenLastCalledWith(WORKSPACE, FLEET, "stop", expect.any(String));
    expect(ctx.writers.settle).toHaveBeenCalledWith(sentId(2));
  });
});

describe("a reused operation id", () => {
  it("test_operation_conflict_offers_no_resend", async () => {
    steerFleetAction.mockResolvedValue(OPERATION_CONFLICT);
    const { ctx, hook } = delivery(() => "optim-1");
    await expect(hook.result.current.onNew(message("deploy"))).rejects.toBeInstanceOf(MessageNotSentError);
    expect(ctx.writers.fail).toHaveBeenCalledWith(sentId(1), PENDING_SEND_STATE.CONFLICT);
  });

  it("test_conflict_draft_mints_a_new_id", async () => {
    steerFleetAction.mockResolvedValueOnce(OPERATION_CONFLICT).mockResolvedValueOnce(ACCEPTED);
    const { ctx, hook } = delivery(() => "optim-1");
    ledgerOf(ctx.writers);
    await expect(hook.result.current.onNew(message("deploy"))).rejects.toBeInstanceOf(MessageNotSentError);
    // The same draft, restored and sent unchanged: a new message, not a replay.
    act(() => hook.result.current.noteRestored(sentId(1), "deploy"));
    await act(() => hook.result.current.onNew(message("deploy")));
    expect(sentId(2)).not.toBe(sentId(1));
  });

  it("sends a conflict's text as new from the notice and dismisses the spent id", async () => {
    steerFleetAction.mockResolvedValueOnce(OPERATION_CONFLICT).mockResolvedValueOnce(ACCEPTED);
    const { ctx, hook } = delivery(() => "optim-1");
    ledgerOf(ctx.writers);
    await expect(hook.result.current.onNew(message("deploy"))).rejects.toBeInstanceOf(MessageNotSentError);
    await act(async () => hook.result.current.resend(sentId(1)));
    expect(ctx.writers.dismiss).toHaveBeenCalledWith(sentId(1));
    expect(sentId(2)).not.toBe(sentId(1));
  });

  it("sends nothing, and dismisses nothing, when no new id can be minted", async () => {
    steerFleetAction.mockResolvedValueOnce(OPERATION_CONFLICT);
    const { ctx, hook } = delivery(() => "optim-1");
    ledgerOf(ctx.writers);
    await expect(hook.result.current.onNew(message("deploy"))).rejects.toBeInstanceOf(MessageNotSentError);
    mint.unavailable = true;
    act(() => hook.result.current.resend(sentId(1)));
    await expect(hook.result.current.onNew(message("deploy"))).rejects.toBeInstanceOf(MessageNotSentError);
    expect(ctx.writers.dismiss).not.toHaveBeenCalled();
    expect(steerFleetAction).toHaveBeenCalledTimes(1);
  });
});
