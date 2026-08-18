import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EVENTS } from "../lib/analytics/events";

// ── Shared mocks ───────────────────────────────────────────────────────────
// Only the server-action module is stubbed; lib/api/runners (HOST_ID_REGEX,
// SANDBOX_TIERS, parseLabels) and lib/errors stay real so the form's own
// client-side validation + error voice are exercised, not faked.

const createRunnerActionMock = vi.fn();
const captureProductEventMock = vi.fn();

vi.mock("@/app/(dashboard)/admin/runners/actions", () => ({
  listRunnersAction: vi.fn(),
  createRunnerAction: createRunnerActionMock,
}));

vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: captureProductEventMock,
}));

// happy-dom ships a real (no-op) navigator.clipboard.writeText; defining a fresh
// object on the instance does not shadow it, so spy on the live method instead.
function stubClipboardWriteText() {
  if (!navigator.clipboard) {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: async () => {} },
      configurable: true,
    });
  }
  return vi.spyOn(navigator.clipboard, "writeText");
}

const MINTED = { ok: true, data: { runner_id: "r1", runner_token: "agt_rdeadbeef" } };

beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("AddRunnerDialog component", () => {
  async function openDialog(onCreated = vi.fn()) {
    const { default: AddRunnerDialog } = await import(
      "../app/(dashboard)/admin/runners/components/AddRunnerDialog"
    );
    // pointerEventsCheck off: Radix's scroll-lock leaves `pointer-events: none`
    // on <body> while the dialog animates; under a loaded shuffled run the check
    // can sample that window and swallow a click INSIDE the dialog content.
    // The dialog's own interactivity is what the assertions prove.
    const user = userEvent.setup({ delay: null, pointerEventsCheck: 0 });
    render(React.createElement(AddRunnerDialog, { onCreated } as never));
    await user.click(screen.getByRole("button", { name: /create runner/i }));
    await waitFor(() => expect(screen.getByLabelText(/host name/i)).toBeTruthy());
    return { user, onCreated };
  }

  async function reachReveal(user: ReturnType<typeof userEvent.setup>) {
    createRunnerActionMock.mockResolvedValue(MINTED);
    await user.type(screen.getByLabelText(/host name/i), "web-prod-1");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: /^create$/i }));
    await screen.findByLabelText("Runner token");
  }

  it("client-side rejects an invalid host id and never calls the action", async () => {
    const { user } = await openDialog();
    await user.type(screen.getByLabelText(/host name/i), "bad host!");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: /^create$/i }));
    await waitFor(() => expect(screen.getByText(/letters, digits, dot, hyphen, underscore/i)).toBeTruthy());
    expect(createRunnerActionMock).not.toHaveBeenCalled();
  });

  it("closes from Cancel before minting and never calls the action", async () => {
    const { user, onCreated } = await openDialog();
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: /^cancel$/i }));
    await waitFor(() => expect(screen.queryByLabelText(/host name/i)).toBeNull());
    expect(createRunnerActionMock).not.toHaveBeenCalled();
    expect(onCreated).not.toHaveBeenCalled();
  });

  it("rejects a malformed label before the round-trip, naming the offender", async () => {
    const { user } = await openDialog();
    await user.type(screen.getByLabelText(/host name/i), "web-prod-1");
    await user.type(screen.getByLabelText(/labels/i), "gpu, bad label!");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: /^create$/i }));
    await waitFor(() => expect(screen.getByText(/must be 1.64 chars/i)).toBeTruthy());
    expect(screen.getByText(/bad label!/)).toBeTruthy();
    expect(createRunnerActionMock).not.toHaveBeenCalled();
  });

  it("happy path: mints with the trimmed host + parsed labels, reveals once, then discards on close", async () => {
    createRunnerActionMock.mockResolvedValue(MINTED);
    const { user, onCreated } = await openDialog();
    await user.type(screen.getByLabelText(/host name/i), "web-prod-1");
    await user.type(screen.getByLabelText(/labels/i), "gpu, us-east, gpu");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: /^create$/i }));

    const field = await screen.findByLabelText("Runner token");
    expect((field as HTMLInputElement).value).toBe("agt_rdeadbeef");
    // host trimmed, labels deduped + parsed, and the FULL assignment envelope
    // at its documented defaults (M148: the dialog assigns policy; the action
    // forwards it verbatim).
    expect(createRunnerActionMock).toHaveBeenCalledWith({
      host_id: "web-prod-1",
      assigned_policy: {
        sandbox_tier: "landlock_full",
        network_policy: "allow_all",
        registry_allowlist: [],
        worker_count: 1,
        // Always sent, explicitly empty — enrollment assigns the whole policy,
        // and an omitted bind list is indistinguishable from "wipe the binds".
        extra_binds: [],
      },
      labels: ["gpu", "us-east"],
    });

    await user.click(screen.getByRole("button", { name: /stored it/i }));
    await waitFor(() => expect(screen.queryByDisplayValue("agt_rdeadbeef")).toBeNull());
    expect(onCreated).toHaveBeenCalled();

    expect(captureProductEventMock).toHaveBeenCalledTimes(1);
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.runner_token_minted, {
      runner_id: "r1",
      sandbox_tier: "landlock_full",
    });
    // The one-time agt_r token must never reach analytics.
    expect(JSON.stringify(captureProductEventMock.mock.calls)).not.toContain("agt_rdeadbeef");
  });

  it("shows the one-time warning only after the runner token is minted", async () => {
    const { user } = await openDialog();
    expect(screen.queryByText(/shown once/i)).toBeNull();

    await reachReveal(user);

    // pin test: the runner warning must match the established one-time-secret voice.
    expect(screen.getByText("Runner token is shown once. Copy it now.")).toBeTruthy();
  });

  it("isolation radiogroup has an accessible name and reads as an assignment, never self-reported", async () => {
    await openDialog();
    // Named via aria-labelledby (a div[role=radiogroup] isn't a labelable
    // HTML element, so FormLabel's htmlFor can't reach it) — not a duplicated
    // literal aria-label string.
    expect(screen.getByRole("radiogroup", { name: /^isolation$/i })).toBeTruthy();
    // M148 Dimension 4.3: the copy describes an assignment the host must
    // satisfy; the pre-inversion self-reported framing is gone.
    expect(screen.getByText(/cannot enforce the assigned tier is degraded/i)).toBeTruthy();
    expect(screen.queryByText(/self-reported/i)).toBeNull();
  });

  it("isolation renders one OptionCard per assignable tier; picking one rides the assignment envelope", async () => {
    createRunnerActionMock.mockResolvedValue(MINTED);
    const { user } = await openDialog();
    const allRadios = screen.getAllByRole("radio");
    // Three assignable tiers (M148 §6 removed the never-enforced Seatbelt one).
    expect(allRadios).toHaveLength(3);
    // Length just asserted above — the indexed access is safe.
    expect(allRadios[0]!.getAttribute("data-state")).toBe("checked"); // default: landlock_full

    await user.click(screen.getByText("Nested container"));
    expect(screen.getByText("Nested container").closest('[role="radio"]')?.getAttribute("data-state")).toBe(
      "checked",
    );

    await user.type(screen.getByLabelText(/host name/i), "web-prod-1");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: /^create$/i }));
    expect(createRunnerActionMock).toHaveBeenCalledWith({
      host_id: "web-prod-1",
      assigned_policy: {
        sandbox_tier: "container_nested",
        network_policy: "allow_all",
        registry_allowlist: [],
        worker_count: 1,
        extra_binds: [],
      },
      labels: [],
    });
  });

  it("a server 403 keeps the dialog open, reveals no token, and does not signal onCreated", async () => {
    createRunnerActionMock.mockResolvedValue({
      ok: false,
      error: "Operator scope required: runner:enroll",
      errorCode: "UZ-AUTH-022",
    });
    const { user, onCreated } = await openDialog();
    await user.type(screen.getByLabelText(/host name/i), "web-prod-1");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: /^create$/i }));
    await waitFor(() => expect(screen.getByText(/additional scope/i)).toBeTruthy());
    expect(screen.queryByLabelText("Runner token")).toBeNull();
    expect(onCreated).not.toHaveBeenCalled();
    expect(captureProductEventMock).not.toHaveBeenCalled();
  });

  it("copies the raw token to the clipboard on demand", async () => {
    const writeText = stubClipboardWriteText().mockResolvedValue(undefined);
    const { user } = await openDialog();
    await reachReveal(user);
    await user.click(screen.getByRole("button", { name: /copy runner token/i }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("agt_rdeadbeef"));
    // findByRole (not sync getByRole) — the "Copied" label flips one microtask
    // after writeText resolves; sync querying races the re-render under load.
    expect(await screen.findByRole("button", { name: /^copied$/i })).toBeTruthy();
  });

  it("falls back to manual selection when the clipboard API is blocked", async () => {
    const writeText = stubClipboardWriteText().mockRejectedValue(new Error("blocked"));
    const { user } = await openDialog();
    await reachReveal(user);
    await user.click(screen.getByRole("button", { name: /copy runner token/i }));
    // Same deterministic contract as the api-key dialog: rejection never
    // flashes success; the exact failed-label proof is the DS CopyButton's.
    await waitFor(() => expect(writeText).toHaveBeenCalled());
    expect(screen.queryByRole("button", { name: /^copied$/i })).toBeNull();
    await waitFor(
      () => expect(screen.getByRole("button", { name: /^copy runner token$/i })).toBeTruthy(),
      { timeout: 4_000 },
    );
    // The reveal stays intact so the operator can still grab the value by hand.
    expect((screen.getByLabelText("Runner token") as HTMLInputElement).value).toBe("agt_rdeadbeef");
  });

  it("selects the whole token on focus so it can be copied manually", async () => {
    const { user } = await openDialog();
    await reachReveal(user);
    const input = screen.getByLabelText("Runner token") as HTMLInputElement;
    const select = vi.spyOn(input, "select");
    fireEvent.focus(input);
    expect(select).toHaveBeenCalled();
  });

  it("keeps the dialog open on Escape while the one-time token is revealed", async () => {
    const { user } = await openDialog();
    await reachReveal(user);
    await user.keyboard("{Escape}");
    // The reveal must survive Escape — the token is shown exactly once.
    expect(screen.getByLabelText("Runner token")).toBeTruthy();
  });

  it("keeps the dialog open on an outside click while the token is revealed", async () => {
    const { user } = await openDialog();
    await reachReveal(user);
    // Radix fires onInteractOutside on a pointerdown outside the content; the
    // overlay-lock must preventDefault so the one-time token isn't lost.
    fireEvent.pointerDown(document.body);
    fireEvent.click(document.body);
    expect(screen.getByLabelText("Runner token")).toBeTruthy();
  });

  it("closes on Escape before a token is minted (no overlay lock yet)", async () => {
    const { user, onCreated } = await openDialog();
    await screen.findByLabelText(/host name/i);
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByLabelText(/host name/i)).toBeNull());
    // Closing before a mint must not fire the parent's refresh.
    expect(onCreated).not.toHaveBeenCalled();
  });

  it("closes on an outside click before a token is minted (no overlay lock yet)", async () => {
    await openDialog();
    await screen.findByLabelText(/host name/i);
    // created is still null → onInteractOutside does NOT preventDefault → closes.
    fireEvent.pointerDown(document.body);
    fireEvent.click(document.body);
    await waitFor(() => expect(screen.queryByLabelText(/host name/i)).toBeNull());
  });
});
