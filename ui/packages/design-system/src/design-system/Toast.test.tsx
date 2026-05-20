import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { Toast } from "./Toast";

afterEach(() => cleanup());

describe("Toast", () => {
  it("renders the children when visible is true", () => {
    render(
      <Toast visible data-testid="t">
        Copied — paste into your terminal
      </Toast>,
    );
    expect(screen.getByTestId("t").textContent).toBe(
      "Copied — paste into your terminal",
    );
  });

  it("renders an empty output element when visible is false (layout slot preserved)", () => {
    render(
      <Toast visible={false} data-testid="t">
        invisible
      </Toast>,
    );
    const el = screen.getByTestId("t");
    expect(el.tagName).toBe("OUTPUT");
    expect((el.textContent ?? "").trim()).toBe("");
  });

  it("emits aria-live=polite + aria-atomic for info severity (default)", () => {
    render(
      <Toast visible data-testid="t">
        info
      </Toast>,
    );
    const el = screen.getByTestId("t");
    expect(el.getAttribute("aria-live")).toBe("polite");
    expect(el.getAttribute("aria-atomic")).toBe("true");
  });

  it("emits aria-live=polite for success severity", () => {
    render(
      <Toast visible severity="success" data-testid="t">
        saved
      </Toast>,
    );
    expect(screen.getByTestId("t").getAttribute("aria-live")).toBe("polite");
  });

  it("escalates to aria-live=assertive for warning severity", () => {
    render(
      <Toast visible severity="warning" data-testid="t">
        clipboard blocked
      </Toast>,
    );
    expect(screen.getByTestId("t").getAttribute("aria-live")).toBe("assertive");
  });

  it("escalates to aria-live=assertive for destructive severity", () => {
    render(
      <Toast visible severity="destructive" data-testid="t">
        save failed
      </Toast>,
    );
    expect(screen.getByTestId("t").getAttribute("aria-live")).toBe("assertive");
  });

  it("applies the severity token to className", () => {
    render(
      <Toast visible severity="success" data-testid="t">
        ok
      </Toast>,
    );
    expect(screen.getByTestId("t").className).toContain("text-success");
  });

  it("preserves caller-supplied className alongside variant classes", () => {
    render(
      <Toast visible className="custom-marker" data-testid="t">
        ok
      </Toast>,
    );
    const cls = screen.getByTestId("t").className;
    expect(cls).toContain("custom-marker");
    expect(cls).toContain("text-text-muted");
  });

  it("drives inline transitionDuration from fadeMs (default 240 ms)", () => {
    render(
      <Toast visible data-testid="t">
        default
      </Toast>,
    );
    expect(screen.getByTestId("t").style.transitionDuration).toBe("240ms");
  });

  it("honors a custom fadeMs by writing it into inline transitionDuration", () => {
    render(
      <Toast visible fadeMs={80} data-testid="t">
        fast
      </Toast>,
    );
    expect(screen.getByTestId("t").style.transitionDuration).toBe("80ms");
  });

  it("toggles opacity classes on visible flip (CSS-only fade trigger)", () => {
    const { rerender } = render(
      <Toast visible data-testid="t">
        on
      </Toast>,
    );
    expect(screen.getByTestId("t").className).toContain("opacity-100");
    rerender(
      <Toast visible={false} data-testid="t">
        on
      </Toast>,
    );
    expect(screen.getByTestId("t").className).toContain("opacity-0");
  });

  it("flips aria-hidden immediately when visible flips false (before fade completes)", () => {
    vi.useFakeTimers();
    const { rerender } = render(
      <Toast visible data-testid="t">
        copied
      </Toast>,
    );
    expect(screen.getByTestId("t").getAttribute("aria-hidden")).toBe("false");
    rerender(
      <Toast visible={false} data-testid="t">
        copied
      </Toast>,
    );
    // aria-hidden flips immediately on the same render; the fade timer
    // is still running but screen readers must not re-announce stale
    // content while the visual fades out.
    expect(screen.getByTestId("t").getAttribute("aria-hidden")).toBe("true");
    vi.useRealTimers();
  });

  it("keeps children mounted during the fade window, then unmounts them", async () => {
    vi.useFakeTimers();
    const { rerender } = render(
      <Toast visible fadeMs={200} data-testid="t">
        copied — paste
      </Toast>,
    );
    expect(screen.getByTestId("t").textContent).toBe("copied — paste");
    rerender(
      <Toast visible={false} fadeMs={200} data-testid="t">
        copied — paste
      </Toast>,
    );
    // Mid-fade (100 ms in): children still mounted so the user sees
    // the text fading rather than snapping to empty.
    await act(async () => {
      vi.advanceTimersByTime(100);
    });
    expect(screen.getByTestId("t").textContent).toBe("copied — paste");
    // Past the fade window (220 ms total > 200 ms fadeMs): children unmount.
    await act(async () => {
      vi.advanceTimersByTime(120);
    });
    expect((screen.getByTestId("t").textContent ?? "").trim()).toBe("");
    vi.useRealTimers();
  });

  it("re-shows children when visible flips true mid-fade (no orphan timer)", async () => {
    vi.useFakeTimers();
    const { rerender } = render(
      <Toast visible fadeMs={200} data-testid="t">
        hello
      </Toast>,
    );
    rerender(
      <Toast visible={false} fadeMs={200} data-testid="t">
        hello
      </Toast>,
    );
    await act(async () => {
      vi.advanceTimersByTime(80);
    });
    // Mid-fade reflip — visible back to true.
    rerender(
      <Toast visible fadeMs={200} data-testid="t">
        hello
      </Toast>,
    );
    // Run the original would-be-orphan timer; children must still be
    // mounted (the cleanup canceled it).
    await act(async () => {
      vi.advanceTimersByTime(500);
    });
    expect(screen.getByTestId("t").textContent).toBe("hello");
    vi.useRealTimers();
  });

  it("clears a pending fade timer when re-shown before the fade resolves", async () => {
    vi.useFakeTimers();
    // Mount hidden so the effect schedules a fade timer (rendered=false path),
    // leaving fadeTimer.current non-null.
    const { rerender } = render(
      <Toast visible={false} fadeMs={200} data-testid="t">
        body
      </Toast>,
    );
    await act(async () => {
      vi.advanceTimersByTime(50);
    });
    // Flip visible BEFORE the 200 ms timer fires. The visible branch must
    // clear + null the in-flight timer (the `if (fadeTimer.current)` arm).
    rerender(
      <Toast visible fadeMs={200} data-testid="t">
        body
      </Toast>,
    );
    expect(screen.getByTestId("t").textContent).toBe("body");
    // Run well past the original 200 ms window; the cleared timer must not
    // fire setRendered(false) and blank the content.
    await act(async () => {
      vi.advanceTimersByTime(500);
    });
    expect(screen.getByTestId("t").textContent).toBe("body");
    vi.useRealTimers();
  });

  it("survives unmount mid-fade without leaking a setTimeout", async () => {
    vi.useFakeTimers();
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const { rerender, unmount } = render(
      <Toast visible fadeMs={200} data-testid="t">
        x
      </Toast>,
    );
    rerender(
      <Toast visible={false} fadeMs={200} data-testid="t">
        x
      </Toast>,
    );
    unmount();
    await act(async () => {
      vi.advanceTimersByTime(500);
    });
    // No setState-on-unmounted-component warnings allowed.
    expect(errSpy).not.toHaveBeenCalled();
    errSpy.mockRestore();
    vi.useRealTimers();
  });
});
