import React from "react";
import { vi } from "vitest";
import { act, render } from "@testing-library/react";

import { TooltipProvider } from "@agentsfleet/design-system";
import { EventDetailsDialog } from "@/components/domain/EventDetailsDialog";
import type { EventDetail } from "@/lib/api/events";
import { serveDetail } from "./event-details-dialog-served";

export const COPY_DIAGNOSTIC_LABEL = "Copy diagnostic";

export function stubClipboardWriteText() {
  if (!navigator.clipboard) {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: async () => {} },
      configurable: true,
    });
  }
  return vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue(undefined);
}

export function event(over: Partial<EventDetail> = {}): EventDetail {
  const now = Date.UTC(2026, 3, 28, 10, 30, 0);
  return {
    event_id: "evt_1",
    fleet_id: "fleet_1",
    workspace_id: "ws_1",
    actor: "github-app",
    event_type: "webhook",
    status: "fleet_error",
    request_json: "{}",
    response_text: null,
    tokens: 1,
    wall_ms: 10,
    cost_nanos: null,
    failure_label: null,
    failure_detail: null,
    checkpoint_id: null,
    resumes_event_id: null,
    created_at: now,
    updated_at: now,
    ...over,
  };
}

/// The tree a test mounts, or hands to `rerender` to swap the open row
/// without serving it a new body.
export function dialogElement(row: EventDetail) {
  return (
    <TooltipProvider>
      <EventDetailsDialog row={row} onOpenChange={vi.fn()} />
    </TooltipProvider>
  );
}

export function renderDialog(row: EventDetail) {
  serveDetail(row);
  return render(dialogElement(row));
}

/// Settle the body fetch. The dialog paints its header and metrics from the
/// row immediately; the request context and the recorded answer arrive a tick
/// later, so a body assertion has to wait for that tick.
export async function renderDialogWithBody(row: EventDetail) {
  let rendered!: ReturnType<typeof renderDialog>;
  await act(async () => {
    rendered = renderDialog(row);
    await Promise.resolve();
  });
  return rendered;
}
