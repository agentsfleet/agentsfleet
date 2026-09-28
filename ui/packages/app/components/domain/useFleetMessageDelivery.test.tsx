import type { AppendMessage } from "@assistant-ui/react";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { PendingSendWriters } from "./useFleetPendingSends";

const steerFleetAction = vi.hoisted(() => vi.fn());
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({ steerFleetAction }));
vi.mock("@/lib/onboarding-refresh", () => ({ requestOnboardingRefresh: vi.fn() }));

const { useMessageDelivery } = await import("./useFleetMessageDelivery");

const WORKSPACE = "ws_delivery";
const FLEET = "fleet_delivery";
const EVENT_ID = "1790573387481-566";
const NO_ROW = "";

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

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
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
