import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { TooltipProvider } from "@agentsfleet/design-system";
import { EventDetailsDialog } from "@/components/domain/EventDetailsDialog";
import type { EventRow } from "@/lib/api/events";

const COPY_DIAGNOSTIC_LABEL = "Copy diagnostic";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function stubClipboardWriteText() {
  if (!navigator.clipboard) {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: async () => {} },
      configurable: true,
    });
  }
  return vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue(undefined);
}

function event(over: Partial<EventRow> = {}): EventRow {
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

function renderDialog(row: EventRow) {
  return render(
    <TooltipProvider>
      <EventDetailsDialog row={row} onOpenChange={vi.fn()} />
    </TooltipProvider>,
  );
}

describe("EventDetailsDialog", () => {
  it("shows the runner's exact response as the failure reason", () => {
    renderDialog(event({
      response_text: "Installed fleet instructions are empty.",
      failure_label: "startup_posture",
    }));

    expect(screen.getByLabelText("Failed event")).toBeTruthy();
    expect(screen.getByText("Installed fleet instructions are empty.")).toBeTruthy();
    expect(screen.queryByText("No specific reason was recorded for this event.")).toBeNull();
    expect(screen.queryByText("Fix")).toBeNull();
    expect(screen.queryByText(
      "Nothing specific can be fixed from this event because it did not record which startup check failed.",
    )).toBeNull();
  });

  it("does not call a specific failure tag an unrecorded reason", () => {
    renderDialog(event({ failure_label: "oom_kill" }));
    expect(screen.getByText("Ran out of memory")).toBeTruthy();
    expect(screen.queryByText("No specific reason was recorded for this event.")).toBeNull();
    expect(screen.queryByText("Fix")).toBeNull();
  });

  it("treats a whitespace-only startup response as unrecorded", () => {
    renderDialog(event({ response_text: "   ", failure_label: "startup_posture" }));
    expect(screen.getByText("Failed a startup safety check")).toBeTruthy();
    expect(screen.getByText("Fix")).toBeTruthy();
  });

  it("shows an unknown recorded failure without inventing startup guidance", () => {
    renderDialog(event({ failure_label: "brand_new_class" }));
    expect(screen.getByText("brand_new_class")).toBeTruthy();
    expect(screen.queryByText("Fix")).toBeNull();
  });

  it("keeps the identifier and relative created time in the header and the copy icon in the footer", () => {
    renderDialog(event({
      event_id: "evt_header",
      actor: "github-app",
      event_type: "webhook",
    }));

    expect(screen.getByRole("heading", { name: "Event details" })).toBeTruthy();
    expect(screen.getByText("ID")).toBeTruthy();
    expect(screen.getByText("evt_header")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Copy event ID" })).toBeTruthy();
    const copyDiagnostic = screen.getByRole("button", { name: COPY_DIAGNOSTIC_LABEL });
    expect(copyDiagnostic.closest("div")?.className).toContain("justify-end");
    expect(screen.queryByText(COPY_DIAGNOSTIC_LABEL)).toBeNull();
    expect(screen.queryByText("Copy event details")).toBeNull();
    expect(screen.getAllByText("Created")).toHaveLength(1);
    expect(screen.queryByText("Updated")).toBeNull();
    const time = document.querySelector("time");
    expect(time?.getAttribute("datetime")).toBe(new Date(event().created_at).toISOString());
    expect(time?.textContent).toMatch(/ago|^in /i);
    expect(screen.queryByText("Event Identifier")).toBeNull();
    expect(screen.queryByText(/Coordinated Universal Time/)).toBeNull();
    expect(screen.queryByText("Status")).toBeNull();
    expect(screen.queryByText("Actor")).toBeNull();
    expect(screen.queryByText("Type")).toBeNull();
  });

  it("formats request context into readable fields and removes internal metadata", () => {
    renderDialog(event({
      actor: "github-app",
      event_type: "webhook",
      request_json: JSON.stringify({
        url: "https://github.com/agentsfleet/agentsfleet/pull/539",
        repo: "agentsfleet/agentsfleet",
        draft: false,
        retried: true,
        number: 539,
        target: null,
        extra_context: { action: "edited" },
      }),
    }));

    const heading = screen.getByRole("heading", { name: "Request context" });
    const section = heading.parentElement?.parentElement;
    if (!section) throw new Error("Request context section was not rendered");
    expect(within(section).getByText("GitHub App")).toBeTruthy();
    expect(within(section).getByText("webhook")).toBeTruthy();
    expect(within(section).getByText("Pull request")).toBeTruthy();
    expect(within(section).getByText("Repository")).toBeTruthy();
    expect(within(section).getByText("Draft")).toBeTruthy();
    expect(within(section).getByText("No")).toBeTruthy();
    expect(within(section).getByText("Yes")).toBeTruthy();
    expect(within(section).getByText("539")).toBeTruthy();
    expect(within(section).getByText("—")).toBeTruthy();
    expect(within(section).getByText("extra context")).toBeTruthy();
    expect(within(section).getByText('{"action":"edited"}')).toBeTruthy();
    expect(within(section).queryByText(/"repo":/)).toBeNull();
    expect(screen.queryByText("Failure tag")).toBeNull();
    expect(screen.queryByText("Event metadata")).toBeNull();
  });

  it("keeps array request context readable", () => {
    renderDialog(event({ request_json: '["opened",482]' }));
    expect(screen.getByText('["opened",482]')).toBeTruthy();
  });

  it("orders failure details before context and the honest next step", () => {
    renderDialog(event({
      event_id: "evt_startup",
      failure_label: "startup_posture",
      request_json: '{"action":"opened"}',
    }));

    const content = screen.getByRole("dialog").textContent ?? "";
    const resultIndex = content.indexOf("Failed a startup safety check");
    const contextIndex = content.indexOf("Request context");
    const fixIndex = content.indexOf("Fix");

    expect(resultIndex).toBeGreaterThanOrEqual(0);
    expect(contextIndex).toBeGreaterThan(resultIndex);
    expect(fixIndex).toBeGreaterThan(contextIndex);
    expect(screen.getByText(
      "Nothing specific can be fixed from this event because it did not record which startup check failed.",
    )).toBeTruthy();
    expect(screen.getByText(
      "Retry it once. If it fails again, use the copy icon below and ask a coding agent to inspect the diagnostic.",
    )).toBeTruthy();
    expect(screen.queryByText("Add non-empty instructions in Skill, then save the fleet.")).toBeNull();
    expect(screen.queryByText("Make an active runner available to this workspace.")).toBeNull();
    expect(screen.queryByText("Select an available model and provider credential.")).toBeNull();
    expect(screen.queryByText("What to check")).toBeNull();
    expect(screen.queryByText(/runner logs/i)).toBeNull();
    expect(screen.queryByText("startup_posture")).toBeNull();
  });

  it("does not repeat the coarse event status in the detail body", () => {
    renderDialog(event({ failure_label: "startup_posture" }));

    const resultAlert = screen.getByLabelText("Failed event").closest("[role='alert']");
    if (!resultAlert) throw new Error("Event result alert was not rendered");
    expect(screen.queryByText("fleet_error", { exact: true })).toBeNull();
    expect(resultAlert.textContent).toBe("Failed a startup safety check");
  });

  it("copies a complete diagnostic payload for a coding agent", async () => {
    const writeText = stubClipboardWriteText();
    renderDialog(event({
      event_id: "evt_copy",
      actor: "github-app",
      event_type: "webhook",
      request_json: '{"action":"opened","pull_request":482}',
      response_text: null,
      failure_label: "startup_posture",
      checkpoint_id: "checkpoint_1",
    }));

    fireEvent.click(screen.getByRole("button", { name: COPY_DIAGNOSTIC_LABEL }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    const copied = writeText.mock.calls[0]?.[0];
    expect(typeof copied).toBe("string");
    const diagnostic: unknown = JSON.parse(copied ?? "{}");
    expect(diagnostic).toMatchObject({
      event_id: "evt_copy",
      status: "fleet_error",
      result: "Failed a startup safety check",
      source: { actor: "github-app", event_type: "webhook" },
      internal_diagnostics: {
        failure_class: "startup_posture",
        checkpoint_id: "checkpoint_1",
      },
    });
    expect(diagnostic).toMatchObject({
      request_context: expect.stringMatching(/omitted.*private or secret/i),
    });
    expect(copied).not.toContain('"pull_request": 482');
  });

  it("shows relative time and exposes the browser timezone on hover", async () => {
    renderDialog(event());
    const time = document.querySelector("time");
    if (!time) throw new Error("Created time was not rendered");

    await userEvent.hover(time);
    const timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
    expect((await screen.findByRole("tooltip")).textContent).toContain(timeZone);
  });

  it("labels an empty browser timezone as local time", async () => {
    vi.spyOn(Intl.DateTimeFormat.prototype, "resolvedOptions").mockReturnValue({
      locale: "en-US",
      calendar: "gregory",
      numberingSystem: "latn",
      timeZone: "",
    });
    renderDialog(event());
    const time = document.querySelector("time");
    if (!time) throw new Error("Created time was not rendered");

    await userEvent.hover(time);
    expect((await screen.findByRole("tooltip")).textContent).toContain("Local time");
  });

  it("uses success and warning icons for their event states", () => {
    const { rerender } = renderDialog(event({
      status: "processed",
      response_text: "Pull request review completed",
    }));
    expect(screen.getByLabelText("Successful event")).toBeTruthy();

    rerender(
      <TooltipProvider>
        <EventDetailsDialog
          row={event({ status: "gate_blocked", response_text: "Waiting for approval" })}
          onOpenChange={vi.fn()}
        />
      </TooltipProvider>,
    );
    expect(screen.getByLabelText("Warning event")).toBeTruthy();

    rerender(
      <TooltipProvider>
        <EventDetailsDialog
          row={event({ status: "weird-unknown", response_text: "Unknown event state" })}
          onOpenChange={vi.fn()}
        />
      </TooltipProvider>,
    );
    expect(screen.getByLabelText("Warning event")).toBeTruthy();
  });

  it("presents a received event as healthy work in progress", () => {
    renderDialog(event({ status: "received", response_text: null }));
    expect(screen.getByLabelText("Event in progress")).toBeTruthy();
    expect(screen.queryByLabelText("Warning event")).toBeNull();
  });

  it("keeps a generic request URL provider-neutral", () => {
    renderDialog(event({
      actor: "webhook:generic",
      request_json: '{"url":"https://example.com/ticket/7"}',
    }));
    expect(screen.getByText("URL")).toBeTruthy();
    expect(screen.queryByText("Pull request")).toBeNull();
  });

  it("bounds a large result in both the dialog and copied diagnostic", async () => {
    const writeText = stubClipboardWriteText();
    const response = `${"x".repeat(20_000)}hidden-result-tail`;
    renderDialog(event({ response_text: response }));

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toHaveLength(20_000);
    expect(alert.textContent?.endsWith("…")).toBe(true);
    expect(screen.queryByText(/hidden-result-tail/)).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: COPY_DIAGNOSTIC_LABEL }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    const copied = writeText.mock.calls[0]?.[0] ?? "";
    expect(copied).not.toContain("hidden-result-tail");
    const diagnostic = JSON.parse(copied) as { recorded_response: string };
    expect(diagnostic.recorded_response).toHaveLength(20_000);
    expect(diagnostic.recorded_response.endsWith("…")).toBe(true);
  });

  it("marks a whitespace-prefixed large result as truncated", () => {
    const response = `   ${"x".repeat(20_000)}hidden-result-tail`;
    renderDialog(event({ response_text: response }));
    const result = screen.getByRole("alert").textContent ?? "";
    expect(result.endsWith("…")).toBe(true);
    expect(result).not.toContain("hidden-result-tail");
  });

  it("preserves an invalid created value in the copied diagnostic", async () => {
    const writeText = stubClipboardWriteText();
    renderDialog(event({ created_at: Number.NaN }));

    fireEvent.click(screen.getByRole("button", { name: COPY_DIAGNOSTIC_LABEL }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    const diagnostic: unknown = JSON.parse(writeText.mock.calls[0]?.[0] ?? "{}");
    expect(diagnostic).toMatchObject({ created_at: "NaN" });
  });

  it("keeps malformed request context visible but omits it from copied diagnostics", async () => {
    const writeText = stubClipboardWriteText();
    renderDialog(event({ request_json: "{not-json" }));
    expect(screen.getByText("{not-json")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: COPY_DIAGNOSTIC_LABEL }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    expect(writeText.mock.calls[0]?.[0]).not.toContain("{not-json");
    expect(writeText.mock.calls[0]?.[0]).toMatch(/omitted.*private or secret/i);
  });

  it("explains when no request context was recorded", () => {
    renderDialog(event({ request_json: "   " }));
    expect(screen.getByText("No request context recorded")).toBeTruthy();
  });

  it("limits rendered request context and omits the hidden tail from copied diagnostics", async () => {
    const writeText = stubClipboardWriteText();
    const visible = "x".repeat(10_000);
    renderDialog(event({ request_json: `${visible}hidden-tail` }));
    const context = screen.getByText(visible);
    expect(context.textContent).toHaveLength(10_000);
    expect(screen.queryByText(/hidden-tail/)).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: COPY_DIAGNOSTIC_LABEL }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    expect(writeText.mock.calls[0]?.[0]).not.toContain("hidden-tail");
    expect(writeText.mock.calls[0]?.[0]).toMatch(/omitted.*private or secret/i);
  });

  it("bounds request-context fields and omits raw values from the copied diagnostic", async () => {
    const writeText = stubClipboardWriteText();
    const request = Object.fromEntries(
      Array.from({ length: 150 }, (_, index) => [`field_${index}`, `value_${index}`]),
    );
    renderDialog(event({ request_json: JSON.stringify(request) }));

    expect(screen.getByText("field 99")).toBeTruthy();
    expect(screen.queryByText("field 100")).toBeNull();
    expect(screen.getByText("Additional fields not shown")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: COPY_DIAGNOSTIC_LABEL }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    expect(writeText.mock.calls[0]?.[0]).not.toContain("value_149");
    expect(writeText.mock.calls[0]?.[0]).toMatch(/omitted.*private or secret/i);
  });
});
