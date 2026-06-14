import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import Terminal from "./Terminal";

describe("Terminal", () => {
  it("renders the command inside a <pre><code>", () => {
    const { container } = render(<Terminal>echo hi</Terminal>);
    const pre = container.querySelector("pre");
    expect(pre).toBeInTheDocument();
    expect(pre?.textContent).toContain("echo hi");
  });

  it("uses operational mono on the deepest surface by default", () => {
    const { container } = render(<Terminal>x</Terminal>);
    const pre = container.querySelector("pre");
    // Body retains the mono + foreground colour contract.
    expect(pre?.className).toContain("text-foreground");
    expect(pre?.className).toContain("font-mono");
    // The terminal-window chrome lives on the outer wrapper now —
    // border + background are owned by the wrapper, not the <pre>.
    const wrapper = container.firstElementChild as HTMLElement | null;
    expect(wrapper?.className).toContain("border-border");
    // The terminal body sits one shade below --bg per the canonical
    // preview (`.cli { background: #06090A; }` line 195) — the Layer 0
    // token is --surface-deep, exposed as `bg-surface-deep`.
    expect(wrapper?.className).toContain("bg-surface-deep");
  });

  it("switches to success colors with green=true", () => {
    const { container } = render(<Terminal green>x</Terminal>);
    const pre = container.querySelector("pre");
    expect(pre?.className).toContain("text-success");
    const wrapper = container.firstElementChild as HTMLElement | null;
    expect(wrapper?.className).toContain("border-success");
  });

  it("renders the terminal-window chrome with three muted dot affordances", () => {
    const { container } = render(<Terminal>x</Terminal>);
    // The chrome is the first child of the wrapper; it carries three
    // <span> dot elements for the window-control affordance.
    const chrome = container.firstElementChild?.firstElementChild as HTMLElement | null;
    expect(chrome).toBeTruthy();
    const dotContainer = chrome?.querySelector("span[aria-hidden='true']");
    expect(dotContainer?.children.length).toBe(3);
  });

  it("renders the label inside the chrome when provided", () => {
    const { container } = render(<Terminal label="Install">x</Terminal>);
    const chrome = container.firstElementChild?.firstElementChild as HTMLElement | null;
    expect(chrome?.textContent).toContain("Install");
  });

  it("does not render a copy button by default", () => {
    render(<Terminal>x</Terminal>);
    expect(screen.queryByTestId("copy-btn")).not.toBeInTheDocument();
  });

  it("renders copy button when copyable=true", () => {
    render(<Terminal copyable>x</Terminal>);
    expect(screen.getByTestId("copy-btn")).toBeInTheDocument();
  });

  it("exposes the command on data-command attribute for string children", () => {
    const { container } = render(<Terminal>my cmd</Terminal>);
    expect(container.querySelector("pre")).toHaveAttribute("data-command", "my cmd");
  });

  it("renders aria-label when label is provided", () => {
    render(<Terminal label="Install">curl ...</Terminal>);
    expect(screen.getByLabelText("Install")).toBeInTheDocument();
  });

  it("omits the data-command attribute when children are JSX, not a string", () => {
    const { container } = render(
      <Terminal>
        <span>echo hi</span>
      </Terminal>,
    );
    expect(container.querySelector("pre")).not.toHaveAttribute("data-command");
  });

  it("hides the copy button when copyable but children are JSX and no copyText is given", () => {
    render(
      <Terminal copyable>
        <span>npm ERR! ENOSPC</span>
      </Terminal>,
    );
    expect(screen.queryByTestId("copy-btn")).not.toBeInTheDocument();
  });

  it("renders the copy button for JSX children once an explicit copyText is supplied", () => {
    render(
      <Terminal copyable copyText="npm install zombie">
        <span>npm ERR! log line</span>
      </Terminal>,
    );
    expect(screen.getByTestId("copy-btn")).toBeInTheDocument();
  });

  describe("copy interaction", () => {
    beforeEach(() => {
      Object.assign(navigator, {
        clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
      });
    });

    it("writes command to clipboard and flips to Copied state on click", async () => {
      render(<Terminal copyable>install command</Terminal>);
      const btn = screen.getByTestId("copy-btn");
      fireEvent.click(btn);
      await waitFor(() =>
        expect(navigator.clipboard.writeText).toHaveBeenCalledWith("install command"),
      );
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Copied!" })).toBeInTheDocument(),
      );
    });

    it("copies the explicit copyText payload over the JSX children", async () => {
      render(
        <Terminal copyable copyText="curl -fsSL agentsfleet.dev | sh">
          <span>colored log</span>
        </Terminal>,
      );
      fireEvent.click(screen.getByTestId("copy-btn"));
      await waitFor(() =>
        expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
          "curl -fsSL agentsfleet.dev | sh",
        ),
      );
    });

    it("clears the in-flight reset timer when copied twice in quick succession", async () => {
      vi.useFakeTimers();
      Object.assign(navigator, {
        clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
      });
      render(<Terminal copyable>cmd</Terminal>);
      const btn = screen.getByTestId("copy-btn");
      // First click schedules the 2s reset timer.
      await act(async () => {
        fireEvent.click(btn);
        await Promise.resolve();
      });
      expect(screen.getByRole("button", { name: "Copied!" })).toBeInTheDocument();
      // Second click before the 2s window — clears the pending timer and
      // reschedules (exercises the `if (resetTimerRef.current) clearTimeout`
      // branch inside the success path).
      await act(async () => {
        vi.advanceTimersByTime(500);
        fireEvent.click(btn);
        await Promise.resolve();
      });
      // Advance past the original 2s mark; still Copied because the timer was reset.
      await act(async () => {
        vi.advanceTimersByTime(1800);
      });
      expect(screen.getByRole("button", { name: "Copied!" })).toBeInTheDocument();
      // Past the rescheduled window — flips back to resting Copy state.
      await act(async () => {
        vi.advanceTimersByTime(400);
      });
      expect(screen.getByRole("button", { name: "Copy command" })).toBeInTheDocument();
      vi.useRealTimers();
    });
  });

  describe("clipboard rejection", () => {
    beforeEach(() => {
      Object.assign(navigator, {
        clipboard: {
          writeText: vi.fn().mockRejectedValue(new Error("denied")),
        },
      });
    });

    it("swallows a rejected clipboard write and leaves the button at rest", async () => {
      const errSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
      render(<Terminal copyable>blocked cmd</Terminal>);
      const btn = screen.getByTestId("copy-btn");
      fireEvent.click(btn);
      await waitFor(() =>
        expect(navigator.clipboard.writeText).toHaveBeenCalledWith("blocked cmd"),
      );
      // The catch arm keeps the button in the resting "Copy command" state —
      // no Copied flash, no unhandled rejection logged.
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Copy command" })).toBeInTheDocument(),
      );
      expect(errSpy).not.toHaveBeenCalled();
      errSpy.mockRestore();
    });
  });

  describe("animate (install-demo line reveal)", () => {
    it("hooks the CSS reveal onto the code block when animate is set", () => {
      const { container } = render(
        <Terminal animate>
          <div>line one</div>
          <div>line two</div>
        </Terminal>,
      );
      const code = container.querySelector("[data-terminal-reveal]");
      expect(code).not.toBeNull();
      // The hook lives on the <code> wrapping the lines; the staggered reveal
      // is driven by tokens.css [data-terminal-reveal] > * (CSS-only).
      expect(code?.tagName).toBe("CODE");
      expect(code?.children).toHaveLength(2);
    });

    it("does not set the reveal hook when animate is absent (static default)", () => {
      const { container } = render(
        <Terminal>
          <div>line one</div>
        </Terminal>,
      );
      expect(container.querySelector("[data-terminal-reveal]")).toBeNull();
    });

    it("renders every line in the DOM regardless of animate (reduced-motion safety is CSS-only)", () => {
      // animate is purely presentational — content is never JS-gated, so under
      // prefers-reduced-motion (handled entirely in tokens.css) every line is
      // present and shown at once.
      render(
        <Terminal animate>
          <div>alpha</div>
          <div>beta</div>
          <div>gamma</div>
        </Terminal>,
      );
      expect(screen.getByText("alpha")).toBeInTheDocument();
      expect(screen.getByText("beta")).toBeInTheDocument();
      expect(screen.getByText("gamma")).toBeInTheDocument();
    });
  });
});
