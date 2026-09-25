import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  COPY_DIAGNOSTIC_LABEL,
  dialogElement,
  event,
  renderDialogWithBody,
  stubClipboardWriteText,
} from "./helpers/event-details-dialog-fixtures";

vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", async () =>
  (await import("./helpers/event-details-dialog-served")).fleetActionsMock(),
);

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("EventDetailsDialog diagnostics", () => {
  it("copies a complete diagnostic payload for a coding agent", async () => {
    const writeText = stubClipboardWriteText();
    await renderDialogWithBody(event({
      event_id: "evt_copy",
      actor: "github-app",
      event_type: "webhook",
      request_json: '{"action":"opened","pull_request":482}',
      response_text: null,
      failure_label: "runner_crash",
      failure_detail: "NoResponseContent: finish_reason=stop stream_frames=0",
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
      result: "This fleet couldn’t complete the reply.",
      source: { actor: "github-app", event_type: "webhook" },
      internal_diagnostics: {
        failure_class: "runner_crash",
        failure_detail: "NoResponseContent: finish_reason=stop stream_frames=0",
        checkpoint_id: "checkpoint_1",
      },
    });
    expect(diagnostic).toMatchObject({
      request_context: expect.stringMatching(/omitted.*private or secret/i),
    });
    expect(copied).not.toContain('"pull_request": 482');
  });

  it("shows relative time and exposes the browser timezone on hover", async () => {
    await renderDialogWithBody(event());
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
    await renderDialogWithBody(event());
    const time = document.querySelector("time");
    if (!time) throw new Error("Created time was not rendered");

    await userEvent.hover(time);
    expect((await screen.findByRole("tooltip")).textContent).toContain("Local time");
  });

  it("uses success and warning icons for their event states", async () => {
    const { rerender } = await renderDialogWithBody(event({
      status: "processed",
      response_text: "Pull request review completed",
    }));
    expect(screen.getByLabelText("Successful event")).toBeTruthy();

    rerender(dialogElement(event({ status: "gate_blocked", response_text: "Waiting for approval" })));
    expect(screen.getByLabelText("Warning event")).toBeTruthy();

    rerender(dialogElement(event({ status: "weird-unknown", response_text: "Unknown event state" })));
    expect(screen.getByLabelText("Warning event")).toBeTruthy();
  });

  it("presents a received event as healthy work in progress", async () => {
    await renderDialogWithBody(event({ status: "received", response_text: null }));
    expect(screen.getByLabelText("Event in progress")).toBeTruthy();
    expect(screen.queryByLabelText("Warning event")).toBeNull();
  });

  it("keeps a generic request URL provider-neutral", async () => {
    await renderDialogWithBody(event({
      actor: "webhook:generic",
      request_json: '{"url":"https://example.com/ticket/7"}',
    }));
    expect(screen.getByText("URL")).toBeTruthy();
    expect(screen.queryByText("Pull request")).toBeNull();
  });

  it("bounds a large result in both the dialog and copied diagnostic", async () => {
    const writeText = stubClipboardWriteText();
    const response = `${"x".repeat(20_000)}hidden-result-tail`;
    await renderDialogWithBody(event({ response_text: response }));

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

  it("marks a whitespace-prefixed large result as truncated", async () => {
    const response = `   ${"x".repeat(20_000)}hidden-result-tail`;
    await renderDialogWithBody(event({ response_text: response }));
    const result = screen.getByRole("alert").textContent ?? "";
    expect(result.endsWith("…")).toBe(true);
    expect(result).not.toContain("hidden-result-tail");
  });

  it("preserves an invalid created value in the copied diagnostic", async () => {
    const writeText = stubClipboardWriteText();
    await renderDialogWithBody(event({ created_at: Number.NaN }));

    fireEvent.click(screen.getByRole("button", { name: COPY_DIAGNOSTIC_LABEL }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    const diagnostic: unknown = JSON.parse(writeText.mock.calls[0]?.[0] ?? "{}");
    expect(diagnostic).toMatchObject({ created_at: "NaN" });
  });

  it("keeps malformed request context visible but omits it from copied diagnostics", async () => {
    const writeText = stubClipboardWriteText();
    await renderDialogWithBody(event({ request_json: "{not-json" }));
    expect(screen.getByText("{not-json")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: COPY_DIAGNOSTIC_LABEL }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    expect(writeText.mock.calls[0]?.[0]).not.toContain("{not-json");
    expect(writeText.mock.calls[0]?.[0]).toMatch(/omitted.*private or secret/i);
  });

  it("explains when no request context was recorded", async () => {
    await renderDialogWithBody(event({ request_json: "   " }));
    expect(screen.getByText("No request context recorded")).toBeTruthy();
  });

  it("limits rendered request context and omits the hidden tail from copied diagnostics", async () => {
    const writeText = stubClipboardWriteText();
    const visible = "x".repeat(10_000);
    await renderDialogWithBody(event({ request_json: `${visible}hidden-tail` }));
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
    await renderDialogWithBody(event({ request_json: JSON.stringify(request) }));

    expect(screen.getByText("field 99")).toBeTruthy();
    expect(screen.queryByText("field 100")).toBeNull();
    expect(screen.getByText("Additional fields not shown")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: COPY_DIAGNOSTIC_LABEL }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    expect(writeText.mock.calls[0]?.[0]).not.toContain("value_149");
    expect(writeText.mock.calls[0]?.[0]).toMatch(/omitted.*private or secret/i);
  });
});
