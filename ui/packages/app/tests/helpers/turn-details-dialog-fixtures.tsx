import React from "react";
import { vi } from "vitest";
import { act, render } from "@testing-library/react";

import { TooltipProvider } from "@agentsfleet/design-system";
import { TurnDetailsDialog } from "@/components/domain/turns/TurnDetailsDialog";
import type { TurnDetail } from "@/lib/api/turns";
import { serveDetail } from "./turn-details-dialog-served";

export function turn(over: Partial<TurnDetail> = {}): TurnDetail {
  const now = Date.UTC(2026, 3, 28, 10, 30, 0);
  return {
    turn_id: "evt_1",
    fleet_id: "fleet_1",
    workspace_id: "ws_1",
    actor: "github-app",
    event_type: "webhook",
    status: "fleet_error",
    request_json: "{}",
    response_text: null,
    tool_calls: null,
    tokens: 1,
    wall_ms: 10,
    cost_nanos: null,
    failure_label: null,
    failure_detail: null,
    resumes_turn_id: null,
    created_at: new Date(now).toISOString(),
    updated_at: new Date(now).toISOString(),
    ...over,
  };
}

/// The tree a test mounts, or hands to `rerender` to swap the open row
/// without serving it a new body.
export function dialogElement(row: TurnDetail) {
  return (
    <TooltipProvider>
      <TurnDetailsDialog row={row} onOpenChange={vi.fn()} />
    </TooltipProvider>
  );
}

export function renderDialog(row: TurnDetail) {
  serveDetail(row);
  return render(dialogElement(row));
}

/// Settle the body fetch. The dialog paints its header and metrics from the
/// row immediately; the request context and the recorded answer arrive a tick
/// later, so a body assertion has to wait for that tick.
export async function renderDialogWithBody(row: TurnDetail) {
  let rendered!: ReturnType<typeof renderDialog>;
  await act(async () => {
    rendered = renderDialog(row);
    await Promise.resolve();
  });
  return rendered;
}
