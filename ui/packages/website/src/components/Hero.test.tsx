import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

const analytics = vi.hoisted(() => ({
  trackNavigationClicked: vi.fn(),
  trackSignupStarted: vi.fn(),
}));

vi.mock("../analytics/posthog", () => analytics);

import Hero from "./Hero";

const INSTALL_COMMAND =
  "npm install -g @usezombie/zombiectl && npx skills add usezombie/usezombie";

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

  it("renders the primary CTA as a terminal-style $ install button (not a docs link)", () => {
    installClipboard();
    renderHero();
    const cta = screen.getByTestId("hero-cta-primary");
    expect(cta.tagName).toBe("BUTTON");
    expect(cta.textContent).toContain(INSTALL_COMMAND);
    expect(cta.textContent).toContain("$");
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

  it("scrolls the #onboarding-flow anchor into view after copying", async () => {
    installClipboard();
    const anchor = document.createElement("section");
    anchor.id = "onboarding-flow";
    const scrollIntoView = vi.fn();
    anchor.scrollIntoView = scrollIntoView;
    document.body.appendChild(anchor);
    try {
      renderHero();
      fireEvent.click(screen.getByTestId("hero-cta-primary"));
      await waitFor(() => expect(scrollIntoView).toHaveBeenCalledTimes(1));
      expect(scrollIntoView.mock.calls[0][0]).toMatchObject({ block: "start" });
    } finally {
      document.body.removeChild(anchor);
    }
  });

  it("honors prefers-reduced-motion: reduce by using behavior='auto' on scroll", async () => {
    installClipboard();
    const anchor = document.createElement("section");
    anchor.id = "onboarding-flow";
    const scrollIntoView = vi.fn();
    anchor.scrollIntoView = scrollIntoView;
    document.body.appendChild(anchor);
    const matchMedia = vi.fn().mockImplementation((query: string) => ({
      matches: query === "(prefers-reduced-motion: reduce)",
      media: query,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
      onchange: null,
    }));
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      writable: true,
      value: matchMedia,
    });
    try {
      renderHero();
      fireEvent.click(screen.getByTestId("hero-cta-primary"));
      await waitFor(() => expect(scrollIntoView).toHaveBeenCalledTimes(1));
      expect(scrollIntoView.mock.calls[0][0]).toMatchObject({ behavior: "auto" });
    } finally {
      document.body.removeChild(anchor);
      Object.defineProperty(window, "matchMedia", {
        configurable: true,
        writable: true,
        value: undefined,
      });
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

  it("renders the secondary CTA pointing at /agents", () => {
    renderHero();
    const cta = screen.getByTestId("hero-cta-secondary");
    expect(cta).toHaveAttribute("href", "/agents");
    expect(cta.textContent).toMatch(/view a real wake/i);
  });

  it("tracks clicks on the secondary CTA", () => {
    renderHero();
    fireEvent.click(screen.getByTestId("hero-cta-secondary"));
    expect(analytics.trackNavigationClicked).toHaveBeenCalledWith({
      source: "hero_secondary_replay",
      surface: "hero",
      target: "agents",
    });
  });

  it("renders the install transcript Terminal", () => {
    renderHero();
    expect(screen.getByTestId("hero-cli")).toBeInTheDocument();
    expect(screen.getByLabelText(/install platform-ops via claude code/i)).toBeInTheDocument();
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
