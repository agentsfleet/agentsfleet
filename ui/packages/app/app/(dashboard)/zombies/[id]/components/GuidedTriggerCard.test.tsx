import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import GuidedTriggerCard from "./GuidedTriggerCard";
import { PROVIDER_GUIDANCE } from "./provider-guidance";

afterEach(() => cleanup());

const trigger = {
  type: "webhook" as const,
  source: "github",
  events: ["workflow_run", "push"],
};

const WEBHOOK = "https://api-dev.usezombie.com/v1/webhooks/zmb_test/github";

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

  it("clears the copied label after the reset window and preserves a newer copied key", async () => {
    vi.useFakeTimers();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderCard();

    fireEvent.click(screen.getByLabelText("Copy registration command"));
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(screen.getByLabelText("Copy registration command").textContent).toMatch(
      /Copied command/,
    );

    // Advance halfway through the first reset window, then trigger a second copy
    // so the two setTimeout callbacks resolve at different times.
    await act(async () => {
      vi.advanceTimersByTime(500);
    });
    fireEvent.click(screen.getByLabelText("Copy webhook URL"));
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(screen.getByLabelText("Copy webhook URL").textContent).toMatch(/Copied URL/);

    // Fire the first (command) timeout — its updater sees copiedKey === "url-shortcut"
    // but its captured key is "command", so the FALSE branch returns k unchanged.
    await act(async () => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.getByLabelText("Copy webhook URL").textContent).toMatch(/Copied URL/);

    // Fire the second (url-shortcut) timeout — copiedKey === key → TRUE branch returns null.
    await act(async () => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.getByLabelText("Copy webhook URL").textContent).toMatch(/Copy URL/);
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
    // User navigates away (or hard-refreshes) before the 1.5 s reset
    // window elapses. The pending timer must be cancelled by the hook's
    // useEffect destructor — no setState on the unmounted tree, no React
    // error logged.
    unmount();
    await act(async () => {
      vi.advanceTimersByTime(5000);
    });
    expect(errSpy).not.toHaveBeenCalled();
    errSpy.mockRestore();
  });

  it("swallows clipboard rejection without throwing", async () => {
    const writeText = vi.fn().mockRejectedValue(new Error("denied"));
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderCard();
    fireEvent.click(screen.getByLabelText("Copy registration command"));
    await waitFor(() => expect(writeText).toHaveBeenCalled());
    // The Copied label never appears — state stayed un-toggled.
    expect(screen.getByLabelText("Copy registration command").textContent).toMatch(
      /Copy command/,
    );
  });
});
