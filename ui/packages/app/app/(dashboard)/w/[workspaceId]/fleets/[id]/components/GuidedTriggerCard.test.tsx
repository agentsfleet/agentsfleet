import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import GuidedTriggerCard from "./GuidedTriggerCard";
import { COPY_RESET_MS } from "@agentsfleet/design-system";
import { PROVIDER_GUIDANCE } from "./provider-guidance";

afterEach(() => cleanup());

const trigger = {
  type: "webhook" as const,
  source: "github",
  events: ["workflow_run", "push"],
};

const WEBHOOK = "https://api-dev.agentsfleet.net/v1/webhooks/agt_test/github";

function renderCard(overrides?: { lastDeliveryAt?: number | null }) {
  return render(
    <GuidedTriggerCard
      trigger={trigger}
      webhookUrl={WEBHOOK}
      guidance={PROVIDER_GUIDANCE.github}
      lastDeliveryAt={overrides?.lastDeliveryAt ?? null}
    />,
  );
}

describe("GuidedTriggerCard", () => {
  it("renders the provider title and the events label", () => {
    renderCard();
    expect(screen.getByText("GitHub")).toBeTruthy();
    expect(screen.getByText("On workflow_run, push")).toBeTruthy();
  });

  it("renders without crashing when the trigger has no `events` field (??-fallback)", () => {
    // Branch coverage: `trigger.events ?? []` right side. Real TRIGGER.md
    // entries may omit `events` to mean "all events"; the provider's
    // `eventsLabel([])` returns a sensible default per provider.
    render(
      <GuidedTriggerCard
        trigger={{ type: "webhook", source: "github" }}
        webhookUrl={WEBHOOK}
        guidance={PROVIDER_GUIDANCE.github}
        lastDeliveryAt={null}
      />,
    );
    expect(screen.getByText("GitHub")).toBeTruthy();
    // GitHub's default eventsLabel for [] is provider-specific; we assert
    // only that the rendered command is non-empty (proves the ??-fallback
    // didn't blow up downstream).
    expect(screen.getByTestId("command-github").textContent?.length).toBeGreaterThan(0);
  });

  it("hides the Variables block when the provider declares zero variables (slack)", () => {
    // Branch coverage: `guidance.variables.length > 0 ? ... : null` false path.
    // Slack's variables list is empty after the M71 P1 fix — pinned here so a
    // future provider table change can't silently re-introduce a dead Variables
    // section.
    render(
      <GuidedTriggerCard
        trigger={{ type: "webhook", source: "slack", events: ["message"] }}
        webhookUrl={`${WEBHOOK.replace("/github", "/slack")}`}
        guidance={PROVIDER_GUIDANCE.slack}
        lastDeliveryAt={null}
      />,
    );
    expect(screen.queryByText(/^Variables$/)).toBeNull();
  });

  it("renders the webhook URL inside a copyable code block", () => {
    renderCard();
    const code = screen.getByTestId("webhook-url");
    expect(code.textContent).toBe(WEBHOOK);
  });

  it("re-renders the rendered command client-side when a variable input changes", () => {
    renderCard();
    const command = screen.getByTestId("command-github");
    expect(command.textContent).toContain("repos/<OWNER>/<REPO>/hooks");
    fireEvent.change(screen.getByLabelText("OWNER"), { target: { value: "acme" } });
    fireEvent.change(screen.getByLabelText("REPO"), { target: { value: "platform" } });
    expect(command.textContent).toContain("repos/acme/platform/hooks");
    expect(command.textContent).toContain(`config[url]=${WEBHOOK}`);
  });

  it("copies the rendered command to the clipboard when the primary CTA is clicked", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderCard();
    fireEvent.click(screen.getByLabelText("Copy registration command"));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    const arg = writeText.mock.calls[0]?.[0] ?? "";
    expect(arg).toContain("gh api -X POST repos/<OWNER>/<REPO>/hooks");
  });

  it("links the deep-link target to the provider's hooks page with the variables substituted", () => {
    renderCard();
    fireEvent.change(screen.getByLabelText("OWNER"), { target: { value: "acme" } });
    fireEvent.change(screen.getByLabelText("REPO"), { target: { value: "platform" } });
    const link = screen.getByRole("link", { name: /open github in a new tab/i });
    expect(link.getAttribute("href")).toBe(
      "https://github.com/acme/platform/settings/hooks/new",
    );
    expect(link.getAttribute("target")).toBe("_blank");
    expect(link.getAttribute("rel")).toBe("noreferrer");
  });

  it("shows 'never' when no last delivery is provided", () => {
    renderCard({ lastDeliveryAt: null });
    expect(screen.getByTestId("last-delivery").textContent).toBe(
      "Last delivery: never",
    );
  });

  it("renders a relative time when a last delivery timestamp is provided", () => {
    renderCard({ lastDeliveryAt: Date.now() - 60_000 });
    const node = screen.getByTestId("last-delivery");
    expect(node.textContent).toMatch(/Last delivery:/);
    expect(node.querySelector("time")).not.toBeNull();
  });

  it("copies the URL via the inline CopyableLine copy button", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderCard();
    fireEvent.click(screen.getAllByLabelText("Copy Webhook URL")[0]!);
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(WEBHOOK));
  });

  it("copies the URL via the shortcut button in the CTA row", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderCard();
    fireEvent.click(screen.getByLabelText("Copy webhook URL"));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(WEBHOOK));
  });

  // Each copy affordance owns its own outcome and its own timer now that they all
  // route through the design-system CopyButton. The card used to share ONE
  // `copiedKey` and one single-shot timer across every button, so copying a second
  // value silently cancelled the first one's reset — coupling the two buttons for
  // no reason. They are independent, and that is what these pin.
  //
  // The accessible name flips to "Copied" on success, so the button is captured
  // BEFORE the click and re-read: re-querying by the idle label would miss the very
  // state change under test. React updates the node in place, so the ref holds.
  it("shows Copied on the button that was copied, and leaves the other alone", async () => {
    vi.useFakeTimers();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderCard();

    const cmd = screen.getByLabelText("Copy registration command");
    const url = screen.getByLabelText("Copy webhook URL");

    fireEvent.click(cmd);
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(cmd.textContent).toMatch(/Copied/);
    // The other button is untouched — no shared state to bleed into it.
    expect(url.textContent).toMatch(/Copy webhook URL/);

    await act(async () => {
      vi.advanceTimersByTime(COPY_RESET_MS);
    });
    expect(cmd.textContent).toMatch(/Copy registration command/);
    vi.useRealTimers();
  });

  it("survives unmount-mid-reset without spurious setState on the unmounted tree (page-navigate / refresh scenario)", async () => {
    vi.useFakeTimers();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const { unmount } = renderCard();
    fireEvent.click(screen.getByLabelText("Copy registration command"));
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    // User navigates away (or hard-refreshes) before the reset window elapses.
    // The pending timer must be cancelled by the hook's useEffect destructor —
    // no setState on the unmounted tree, no React error logged.
    unmount();
    await act(async () => {
      vi.advanceTimersByTime(5000);
    });
    expect(errSpy).not.toHaveBeenCalled();
    errSpy.mockRestore();
    vi.useRealTimers();
  });

  // This card used to swallow a clipboard rejection outright. It no longer does:
  // the button reports the failure, because a copy that silently did nothing on a
  // button that looked like it worked sends the user off with an empty clipboard.
  it("reports a clipboard rejection rather than swallowing it", async () => {
    const writeText = vi.fn().mockRejectedValue(new Error("denied"));
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderCard();
    const cmd = screen.getByLabelText("Copy registration command");
    fireEvent.click(cmd);
    await waitFor(() => expect(writeText).toHaveBeenCalled());
    await waitFor(() => expect(cmd.textContent).toMatch(/Copy failed/i));
    // Never the success flash: that is the lie this branch exists to prevent.
    expect(cmd.textContent).not.toMatch(/^Copied$/);
  });
});
