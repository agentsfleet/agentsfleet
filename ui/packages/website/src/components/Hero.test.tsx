import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

const analytics = vi.hoisted(() => ({
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));

vi.mock("../analytics/posthog", () => analytics);

import Hero from "./Hero";

const INSTALL_COMMAND = "curl -fsSL https://usezombie.sh | bash";
const INSTALL_SKILL_COMMAND = "claude /usezombie-install-platform-ops";

function renderHero() {
  return render(
    <BrowserRouter>
      <Hero />
    </BrowserRouter>
  );
}

function installClipboard() {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
  return writeText;
}

describe("Hero", () => {
  beforeEach(() => {
    analytics.trackNavigationClicked.mockReset();
    analytics.trackSignupStarted.mockReset();
  });

  it("renders the two-line mono headline", () => {
    const { container } = renderHero();
    const h1 = container.querySelector("h1");
    expect(h1).not.toBeNull();
    expect(h1).toHaveTextContent(/your deploy failed/i);
    expect(h1).toHaveTextContent(/the agent already knows why/i);
    expect(h1!.className).toContain("font-mono");
  });

  it("renders the LIVE eyebrow with a WakePulse data-live=true mark", () => {
    renderHero();
    const eyebrow = screen.getByTestId("hero-eyebrow");
    expect(eyebrow.textContent).toMatch(/LIVE — wake\.on\.event/i);
    const pulse = eyebrow.querySelector("[data-live=\"true\"]");
    expect(pulse).not.toBeNull();
  });

  it("renders the lede paragraph in the spec voice", () => {
    renderHero();
    expect(screen.getByText(/long-lived runtime that owns one operational outcome/i)).toBeInTheDocument();
    expect(screen.getByText(/durable, replayable log/i)).toBeInTheDocument();
  });

  it("renders the install command in a copy-row with a copy-only Copy button", () => {
    installClipboard();
    renderHero();
    // The long command lives in its own copy-row, not inside the button.
    const command = screen.getByTestId("hero-install-command");
    expect(command.textContent).toContain(INSTALL_COMMAND);
    const cta = screen.getByTestId("hero-cta-primary");
    expect(cta.tagName).toBe("BUTTON");
    expect(cta.textContent).toMatch(/copy/i);
    expect(cta.getAttribute("href")).toBeNull();
  });

  it("writes the install command to the clipboard on primary CTA click", async () => {
    const writeText = installClipboard();
    renderHero();
    fireEvent.click(screen.getByTestId("hero-cta-primary"));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    expect(writeText.mock.calls[0][0]).toBe(INSTALL_COMMAND);
    expect(analytics.trackSignupStarted).toHaveBeenCalledWith({
      source: "hero_primary",
      surface: "hero",
      mode: "humans",
    });
  });

  it("copies only — does not scroll/navigate on primary CTA click (no #onboarding-flow jump)", async () => {
    const writeText = installClipboard();
    // A real #onboarding-flow anchor exists on the page; the old hero scrolled
    // to it on click. The copy-row must NOT — that was the "jumps to a different
    // page" bug. Clicking copies and stays put.
    const anchor = document.createElement("section");
    anchor.id = "onboarding-flow";
    const scrollIntoView = vi.fn();
    anchor.scrollIntoView = scrollIntoView;
    document.body.appendChild(anchor);
    try {
      renderHero();
      fireEvent.click(screen.getByTestId("hero-cta-primary"));
      await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
      expect(scrollIntoView).not.toHaveBeenCalled();
    } finally {
      document.body.removeChild(anchor);
    }
  });

  it("shows the copied toast then dismisses it after the visible window", async () => {
    vi.useFakeTimers();
    installClipboard();
    renderHero();
    await act(async () => {
      fireEvent.click(screen.getByTestId("hero-cta-primary"));
      // Two microtask flushes — one for clipboard.writeText resolve, one
      // for the React state update that paints the toast.
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(screen.getByTestId("hero-cta-toast").textContent).toMatch(
      /Copied — paste into your terminal/i,
    );
    // Toast arms its fade-out unmount timer only after the `visible=false`
    // render commits, and React flushes that passive effect at the act()
    // boundary. So cross the 2 s visible window in one act (which arms the
    // 240 ms unmount timer), then advance through the fade in a second act
    // to fire it. The two act() boundaries are the synchronization points —
    // this is deterministic, not a timing race on a single advance.
    await act(async () => {
      vi.advanceTimersByTime(2000);
    });
    await act(async () => {
      vi.advanceTimersByTime(1000);
    });
    expect((screen.getByTestId("hero-cta-toast").textContent ?? "").trim()).toBe("");
  });

  it("keeps the toast text mounted through the fade-out after the visible window ends", async () => {
    vi.useFakeTimers();
    installClipboard();
    renderHero();
    await act(async () => {
      fireEvent.click(screen.getByTestId("hero-cta-primary"));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(screen.getByTestId("hero-cta-toast").textContent).toMatch(
      /Copied — paste into your terminal/i,
    );
    // Land inside the fade-out: 2100 ms is past the 2 s visible window (so
    // `visible` is false and the fade is running) but short of the 240 ms
    // fade unmount. The text must still be mounted so it fades visibly
    // rather than snapping to empty the same paint the toast hides — Hero
    // keeps passing the message after `toast` clears via the last-shown ref.
    await act(async () => {
      vi.advanceTimersByTime(2100);
    });
    expect(screen.getByTestId("hero-cta-toast").textContent).toMatch(
      /Copied — paste into your terminal/i,
    );
  });

  it("survives unmount-mid-toast without spurious setState (page-navigate / refresh scenario)", async () => {
    vi.useFakeTimers();
    installClipboard();
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const { unmount } = renderHero();
    fireEvent.click(screen.getByTestId("hero-cta-primary"));
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

  it("falls back to the manual-copy toast when the clipboard API rejects", async () => {
    const writeText = vi.fn().mockRejectedValue(new Error("blocked"));
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderHero();
    fireEvent.click(screen.getByTestId("hero-cta-primary"));
    await waitFor(() =>
      expect(screen.getByTestId("hero-cta-toast").textContent).toMatch(
        /Clipboard blocked/i,
      ),
    );
  });

  it("renders the manual toast as a warning and holds that severity through the fade-out", async () => {
    // The fade-out fix derives BOTH text and severity from the last-shown
    // kind. This pins the severity half: a warning toast must stay a warning
    // (assertive live region) while it fades, not silently revert to the
    // info/polite default the same paint `toast` clears.
    vi.useFakeTimers();
    const writeText = vi.fn().mockRejectedValue(new Error("blocked"));
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderHero();
    await act(async () => {
      fireEvent.click(screen.getByTestId("hero-cta-primary"));
      await Promise.resolve();
      await Promise.resolve();
    });
    const toast = screen.getByTestId("hero-cta-toast");
    expect(toast.textContent).toMatch(/Clipboard blocked/i);
    // warning → assertive (info would be polite).
    expect(toast.getAttribute("aria-live")).toBe("assertive");
    // Past the 2 s visible window, inside the 240 ms fade: severity holds.
    await act(async () => {
      vi.advanceTimersByTime(2100);
    });
    const fading = screen.getByTestId("hero-cta-toast");
    expect(fading.textContent).toMatch(/Clipboard blocked/i);
    expect(fading.getAttribute("aria-live")).toBe("assertive");
  });

  it("renders no replay link — the install copy-row is the only primary action", () => {
    renderHero();
    expect(screen.queryByTestId("hero-cta-secondary")).toBeNull();
    expect(screen.queryByText(/view a real wake/i)).toBeNull();
    // The install copy-row and the animated install terminal remain.
    expect(screen.getByTestId("hero-install-command")).toBeInTheDocument();
    expect(screen.getByTestId("hero-cli")).toBeInTheDocument();
  });

  it("renders the animated install Terminal whose Copy yields only the slash command", () => {
    const writeText = installClipboard();
    renderHero();
    const cli = screen.getByTestId("hero-cli");
    expect(cli).toBeInTheDocument();
    expect(screen.getByLabelText(/install via usezombie\.sh/i)).toBeInTheDocument();
    // `animate` hooks the CSS reveal onto the code block.
    expect(cli.querySelector("[data-terminal-reveal]")).not.toBeNull();
    // Copy hands back the next-step slash command, not the whole transcript.
    fireEvent.click(within(cli).getByTestId("copy-btn"));
    expect(writeText).toHaveBeenCalledWith(INSTALL_SKILL_COMMAND);
  });

  it("does not render orange-era hero scaffolding", () => {
    const { container } = renderHero();
    expect(container.querySelector(".hero-illustration")).toBeNull();
    expect(container.querySelector(".hero-proof-grid")).toBeNull();
    expect(container.querySelector(".hero-cta-primary")).toBeNull();
    expect(container.querySelector(".hero-headline")).toBeNull();
  });

  it("renders the promo pill as a link to /pricing with the rates-pin trial-end string", () => {
    renderHero();
    const pill = screen.getByTestId("hero-promo-pill");
    expect(pill.tagName).toBe("A");
    expect(pill).toHaveAttribute("href", "/pricing");
    // Sourced from RATES_DISPLAY.FREE_TRIAL_PILL in lib/rates.ts; rates.test.ts
    // pins the literal — this assertion catches accidental hardcoding in Hero.
    expect(pill.textContent).toMatch(/Free until July 31, 2026/);
    expect(pill.textContent).toMatch(/Promo/);
  });

  it("places the promo pill after the eyebrow and before the headline in document order", () => {
    renderHero();
    const eyebrow = screen.getByTestId("hero-eyebrow");
    const pill = screen.getByTestId("hero-promo-pill");
    const headline = screen.getByTestId("hero-headline");
    expect(
      eyebrow.compareDocumentPosition(pill) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      pill.compareDocumentPosition(headline) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("tracks clicks on the promo pill", () => {
    renderHero();
    fireEvent.click(screen.getByTestId("hero-promo-pill"));
    expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({
      source: "hero_promo_pill",
      surface: "hero",
      target: "pricing",
    });
  });
});
