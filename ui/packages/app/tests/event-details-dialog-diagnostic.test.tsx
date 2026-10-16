import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  dialogElement,
  event,
  renderDialogWithBody,
} from "./helpers/event-details-dialog-fixtures";

vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", async () =>
  (await import("./helpers/event-details-dialog-served")).fleetActionsMock(),
);

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("EventDetailsDialog diagnostics", () => {

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

  it("bounds a large result in the dialog", async () => {
    const response = `${"x".repeat(20_000)}hidden-result-tail`;
    await renderDialogWithBody(event({ response_text: response }));

    const alert = screen.getByRole("alert");
    expect(alert.textContent).toHaveLength(20_000);
    expect(alert.textContent?.endsWith("…")).toBe(true);
    expect(screen.queryByText(/hidden-result-tail/)).toBeNull();


  });

  it("marks a whitespace-prefixed large result as truncated", async () => {
    const response = `   ${"x".repeat(20_000)}hidden-result-tail`;
    await renderDialogWithBody(event({ response_text: response }));
    const result = screen.getByRole("alert").textContent ?? "";
    expect(result.endsWith("…")).toBe(true);
    expect(result).not.toContain("hidden-result-tail");
  });


  it("keeps malformed request context visible", async () => {
    await renderDialogWithBody(event({ request_json: "{not-json" }));
    expect(screen.getByText("{not-json")).toBeTruthy();


  });

  it("explains when no request context was recorded", async () => {
    await renderDialogWithBody(event({ request_json: "   " }));
    expect(screen.getByText("No request context recorded")).toBeTruthy();
  });

  it("limits rendered request context without exposing its hidden tail", async () => {
    const visible = "x".repeat(10_000);
    await renderDialogWithBody(event({ request_json: `${visible}hidden-tail` }));
    const context = screen.getByText(visible);
    expect(context.textContent).toHaveLength(10_000);
    expect(screen.queryByText(/hidden-tail/)).toBeNull();


  });

  it("bounds rendered request-context fields", async () => {
    const request = Object.fromEntries(
      Array.from({ length: 150 }, (_, index) => [`field_${index}`, `value_${index}`]),
    );
    await renderDialogWithBody(event({ request_json: JSON.stringify(request) }));

    expect(screen.getByText("field 99")).toBeTruthy();
    expect(screen.queryByText("field 100")).toBeNull();
    expect(screen.getByText("Additional fields not shown")).toBeTruthy();


  });
});
