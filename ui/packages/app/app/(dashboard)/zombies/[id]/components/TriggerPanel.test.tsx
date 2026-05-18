import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import TriggerPanel, { triggerKey } from "./TriggerPanel";
import type { ZombieTrigger } from "@/lib/types";

afterEach(() => cleanup());

const githubTrigger: ZombieTrigger = {
  type: "webhook",
  source: "github",
  events: ["workflow_run"],
};
const cronTrigger: ZombieTrigger = { type: "cron", schedule: "*/15 * * * *" };
const weirdcoTrigger: ZombieTrigger = { type: "webhook", source: "weirdco" };
const triggers: ZombieTrigger[] = [githubTrigger, cronTrigger, weirdcoTrigger];

describe("TriggerPanel", () => {
  it("renders the empty-state when no triggers are declared", () => {
    render(<TriggerPanel zombieId="zmb_x" />);
    expect(screen.getByTestId("trigger-panel-empty")).toBeTruthy();
    expect(screen.getByText(/No triggers declared/i)).toBeTruthy();
    // The legacy bare webhook URL is still surfaced as a fallback ingress.
    expect(screen.getByTestId("webhook-url").textContent).toBe(
      "https://api-dev.usezombie.com/v1/webhooks/zmb_x",
    );
  });

  it("renders one accordion item per trigger in declared order", () => {
    render(<TriggerPanel zombieId="zmb_x" triggers={triggers} />);
    expect(screen.getByTestId("trigger-label-webhook:github").textContent).toMatch(
      /Webhook · github/,
    );
    expect(screen.getByTestId("trigger-label-cron:*/15 * * * *").textContent).toMatch(
      /Cron · \*\/15/,
    );
    expect(screen.getByTestId("trigger-label-webhook:weirdco").textContent).toMatch(
      /Webhook · weirdco/,
    );
  });

  it("falls back to the copy-URL card when the source has no provider-guidance", async () => {
    render(<TriggerPanel zombieId="zmb_x" triggers={triggers} />);
    // Expand the weirdco accordion item.
    fireEvent.click(screen.getByText(/Webhook · weirdco/i));
    await waitFor(() =>
      expect(screen.getByTestId("copy-url-fallback-weirdco")).toBeTruthy(),
    );
    expect(screen.getByTestId("copy-url-fallback-weirdco").textContent).toMatch(
      /Unknown provider — paste this URL into any webhook-capable service\./,
    );
    expect(screen.getByTestId("webhook-url").textContent).toBe(
      "https://api-dev.usezombie.com/v1/webhooks/zmb_x/weirdco",
    );
  });

  it("renders the API-ingress helper line on the api trigger card (not the unknown-provider copy)", async () => {
    render(<TriggerPanel zombieId="zmb_x" triggers={[{ type: "api" }]} />);
    // Single api trigger with no recorded delivery → auto-expands on mount.
    await waitFor(() => expect(screen.getByTestId("copy-url-fallback-api")).toBeTruthy());
    const card = screen.getByTestId("copy-url-fallback-api");
    expect(card.textContent).toMatch(/API ingress — POST events directly to this URL\./);
    expect(card.textContent).not.toMatch(/Unknown provider/);
  });

  it("renders the bare-webhook helper line on the empty-triggers fallback (no source)", async () => {
    render(<TriggerPanel zombieId="zmb_x" />);
    const card = screen.getByTestId("copy-url-fallback-none");
    expect(card.textContent).toMatch(/Bare webhook URL — POST events here from any service\./);
    expect(card.textContent).not.toMatch(/Unknown provider/);
  });

  it("falls back to the Unknown-provider line when the source name collides with an Object.prototype key", async () => {
    // Hardening: `COPY_URL_FALLBACK_HELPER_TEXT[source]` would inherit
    // Object.prototype members (e.g. `constructor`, `toString`) if the
    // lookup did not gate on Object.hasOwn — rendering `function Object()
    // { [native code] }` as helper text instead of the fallback line.
    render(
      <TriggerPanel
        zombieId="zmb_x"
        triggers={[{ type: "webhook", source: "constructor" } as never]}
      />,
    );
    await waitFor(() =>
      expect(screen.getByTestId("copy-url-fallback-constructor")).toBeTruthy(),
    );
    const card = screen.getByTestId("copy-url-fallback-constructor");
    expect(card.textContent).toMatch(
      /Unknown provider — paste this URL into any webhook-capable service\./,
    );
    expect(card.textContent).not.toMatch(/\[native code\]/);
  });

  it("collapses an open accordion item when the user clicks the trigger again", () => {
    // Branch coverage: `onValueChange={(v) => setOpenValue(v || undefined)}`
    // — the user-collapse path where Radix passes `""` and we coerce to
    // `undefined`. Auto-expand paths only exercise the expand half.
    render(<TriggerPanel zombieId="zmb_x" triggers={[githubTrigger]} />);
    const triggerBtn = screen.getByRole("button", { name: /Webhook · github/i });
    expect(triggerBtn.getAttribute("aria-expanded")).toBe("true"); // auto-expanded
    fireEvent.click(triggerBtn);
    expect(triggerBtn.getAttribute("aria-expanded")).toBe("false");
  });

  it("renders the never-delivered badge when lastDeliveryByKey reports null", () => {
    const map = {
      "webhook:github": null,
      "cron:*/15 * * * *": null,
      "webhook:weirdco": null,
    };
    render(<TriggerPanel zombieId="zmb_x" triggers={triggers} lastDeliveryByKey={map} />);
    const badges = screen.getAllByTestId("last-delivery-badge");
    expect(badges.every((b) => b.textContent === "never")).toBe(true);
  });

  it("renders a <time> relative-delivery badge when lastDeliveryByKey reports an epoch", () => {
    const map = { "webhook:github": Date.now() - 60_000 };
    render(<TriggerPanel zombieId="zmb_x" triggers={[githubTrigger]} lastDeliveryByKey={map} />);
    const badge = screen.getByTestId("last-delivery-badge");
    expect(badge.querySelector("time")).not.toBeNull();
  });

  it("copies the webhook URL when the fallback card's copy button is clicked", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    render(<TriggerPanel zombieId="zmb_x" />);
    fireEvent.click(screen.getByLabelText("Copy webhook URL"));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    const arg = writeText.mock.calls[0]?.[0] ?? "";
    expect(arg).toBe("https://api-dev.usezombie.com/v1/webhooks/zmb_x");
  });

  it("produces stable accordion keys via triggerKey()", () => {
    expect(triggerKey({ type: "webhook", source: "github" })).toBe("webhook:github");
    expect(triggerKey({ type: "cron", schedule: "*/15 * * * *" })).toBe(
      "cron:*/15 * * * *",
    );
    expect(triggerKey({ type: "api" })).toBe("api");
  });

  it("labels the api accordion row 'API ingress'", () => {
    render(
      <TriggerPanel
        zombieId="zmb_x"
        triggers={[{ type: "api" }]}
      />,
    );
    expect(screen.getByTestId("trigger-label-api").textContent).toBe("API ingress");
  });

  it("omits the last-delivery badge when the parent passes no map entry for a trigger", () => {
    render(<TriggerPanel zombieId="zmb_x" triggers={[githubTrigger]} lastDeliveryByKey={{}} />);
    expect(screen.queryByTestId("last-delivery-badge")).toBeNull();
  });

  it("auto-expands a cron trigger that has no recorded delivery and renders the CronCard", async () => {
    render(
      <TriggerPanel
        zombieId="zmb_x"
        triggers={[cronTrigger]}
        lastDeliveryByKey={{ "cron:*/15 * * * *": null }}
      />,
    );
    await waitFor(() => expect(screen.getByTestId("cron-card")).toBeTruthy());
  });

  it("CopyUrlFallback survives unmount-mid-reset without spurious setState (page-navigate / refresh scenario)", async () => {
    vi.useFakeTimers();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const { unmount } = render(<TriggerPanel zombieId="zmb_x" />);
    fireEvent.click(screen.getByLabelText("Copy webhook URL"));
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    unmount();
    await act(async () => {
      vi.advanceTimersByTime(5000);
    });
    expect(errSpy).not.toHaveBeenCalled();
    errSpy.mockRestore();
  });

  it("auto-expands an api trigger that has no recorded delivery and renders the copy-URL fallback", async () => {
    render(
      <TriggerPanel
        zombieId="zmb_x"
        triggers={[{ type: "api" }]}
        lastDeliveryByKey={{ api: null }}
      />,
    );
    await waitFor(() => expect(screen.getByTestId("copy-url-fallback-api")).toBeTruthy());
    expect(screen.getByTestId("webhook-url").textContent).toBe(
      "https://api-dev.usezombie.com/v1/webhooks/zmb_x",
    );
  });
});
