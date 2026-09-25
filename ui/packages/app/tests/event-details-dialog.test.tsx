import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, within } from "@testing-library/react";

import { GUIDANCE, OUTCOME } from "@/lib/events/event-summary";
import { COPY_DIAGNOSTIC_LABEL, event, renderDialogWithBody } from "./helpers/event-details-dialog-fixtures";

vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", async () =>
  (await import("./helpers/event-details-dialog-served")).fleetActionsMock(),
);

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("EventDetailsDialog", () => {
  it("states no reply only when the row affirmatively carries none", async () => {
    // The dialog is the one surface that holds the body, so it is the one
    // surface allowed to say a reply is absent — and it says so from the body,
    // in its own words, never from a status. A processed row with no body
    // shows the absence marker; the completion sentence the list surfaces use
    // is not a claim about the reply and does not appear here.
    await renderDialogWithBody(event({ status: "processed", response_text: null, failure_label: null }));
    // pin test: the literal is the contract the dialog renders
    expect(screen.getByText("No result recorded")).toBeTruthy();
    expect(screen.queryByText(OUTCOME.COMPLETED)).toBeNull();
    expect(screen.queryByText(/no reply/i)).toBeNull();
  });

  it("shows the runner's exact response as the failure reason", async () => {
    await renderDialogWithBody(event({
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

  it("does not call a specific failure tag an unrecorded reason", async () => {
    await renderDialogWithBody(event({ failure_label: "oom_kill" }));
    expect(screen.getByText("Ran out of memory")).toBeTruthy();
    expect(screen.queryByText("No specific reason was recorded for this event.")).toBeNull();
    expect(screen.queryByText("Fix")).toBeNull();
  });

  it("treats a whitespace-only startup response as unrecorded", async () => {
    await renderDialogWithBody(event({ response_text: "   ", failure_label: "startup_posture" }));
    expect(screen.getByText("Failed a startup safety check")).toBeTruthy();
    expect(screen.getByText("Fix")).toBeTruthy();
  });

  it("shows an unknown recorded failure without inventing startup guidance", async () => {
    await renderDialogWithBody(event({ failure_label: "brand_new_class" }));
    expect(screen.getByText("brand_new_class")).toBeTruthy();
    expect(screen.queryByText("Fix")).toBeNull();
  });

  it("keeps the identifier and relative created time in the header and the copy icon in the footer", async () => {
    await renderDialogWithBody(event({
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

  it("formats request context into readable fields and removes internal metadata", async () => {
    await renderDialogWithBody(event({
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

  it("keeps array request context readable", async () => {
    await renderDialogWithBody(event({ request_json: '["opened",482]' }));
    expect(screen.getByText('["opened",482]')).toBeTruthy();
  });

  it("orders failure details before context and the honest next step", async () => {
    await renderDialogWithBody(event({
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
    // No recorded cause: the guidance still names the actionable surface, and
    // the older fall-back advice stays because nothing here says WHICH check.
    expect(screen.getByText(GUIDANCE.STARTUP)).toBeTruthy();
    expect(screen.getByText(/did not record which check failed/)).toBeTruthy();
    expect(screen.queryByText("Add non-empty instructions in Skill, then save the fleet.")).toBeNull();
    expect(screen.queryByText("Make an active runner available to this workspace.")).toBeNull();
    expect(screen.queryByText("Select an available model and provider credential.")).toBeNull();
    expect(screen.queryByText("What to check")).toBeNull();
    expect(screen.queryByText(/runner logs/i)).toBeNull();
    expect(screen.queryByText("startup_posture")).toBeNull();
  });

  it("shows the recorded cause in full and drops the no-cause advice", async () => {
    const cause = "startup check 'instructions' failed: no instructions configured";
    await renderDialogWithBody(event({ failure_label: "startup_posture", failure_detail: cause }));

    // Inspect is where the whole stored value is readable — sentence AND cause.
    const content = screen.getByRole("dialog").textContent ?? "";
    expect(content).toContain("Failed a startup safety check");
    expect(content).toContain(cause);
    // The cause names the check, so the "we don't know which check" advice
    // would now be a lie; only the actionable guidance remains.
    expect(screen.getByText(GUIDANCE.STARTUP)).toBeTruthy();
    expect(screen.queryByText(/did not record which check failed/)).toBeNull();
  });

  it("renders no guidance for a failure class the operator cannot act on", async () => {
    await renderDialogWithBody(event({ failure_label: "oom_kill", failure_detail: "child exceeded its memory cap" }));

    expect(screen.queryByText(GUIDANCE.STARTUP)).toBeNull();
    expect(screen.queryByText("Fix")).toBeNull();
  });

  it("shows a provider refusal as a runner diagnostic without suggesting a blind retry", async () => {
    const cause = "ApiError: compatible: status=404 message=Model not found";
    await renderDialogWithBody(event({ failure_label: "runner_crash", failure_detail: cause }));

    expect(screen.getByText("This fleet couldn’t complete the reply.")).toBeTruthy();
    const heading = screen.getByRole("heading", { name: "Runner diagnostic" });
    expect(heading.parentElement?.textContent).toContain(cause);
    expect(screen.queryByText("Fix")).toBeNull();
  });

  it("does not show an empty runner diagnostic", async () => {
    await renderDialogWithBody(event({ failure_label: "runner_crash", failure_detail: null }));

    expect(screen.queryByRole("heading", { name: "Runner diagnostic" })).toBeNull();
  });

  it("renders no guidance when the fleet recorded a real reply", async () => {
    await renderDialogWithBody(event({
      failure_label: "startup_posture",
      failure_detail: "no instructions configured",
      response_text: "I recovered and reviewed the pull request.",
    }));

    // The fleet's own words outrank a canned remediation line.
    expect(screen.queryByText(GUIDANCE.STARTUP)).toBeNull();
  });

  it("does not repeat the coarse event status in the detail body", async () => {
    await renderDialogWithBody(event({ failure_label: "startup_posture" }));

    const resultAlert = screen.getByLabelText("Failed event").closest("[role='alert']");
    if (!resultAlert) throw new Error("Event result alert was not rendered");
    expect(screen.queryByText("fleet_error", { exact: true })).toBeNull();
    expect(resultAlert.textContent).toBe("Failed a startup safety check");
  });
});
