import { render, screen, fireEvent, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { Terminal } from "@usezombie/design-system";

const mockWriteText = vi.fn().mockResolvedValue(undefined);

beforeEach(() => {
  vi.clearAllMocks();
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText: mockWriteText },
    writable: true,
    configurable: true,
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("Terminal — basic", () => {
  it("renders children as code", () => {
    render(<Terminal>npm install</Terminal>);
    expect(screen.getByText("npm install")).toBeInTheDocument();
  });

  it("has z-terminal class on the pre element", () => {
    const { container } = render(<Terminal>cmd</Terminal>);
    expect(container.querySelector(".z-terminal")).toBeInTheDocument();
  });

  it("does not have green class by default", () => {
    const { container } = render(<Terminal>cmd</Terminal>);
    expect(container.querySelector(".z-terminal")).not.toHaveClass("z-terminal--green");
  });

  it("adds green class when green=true", () => {
    const { container } = render(<Terminal green>cmd</Terminal>);
    expect(container.querySelector(".z-terminal")).toHaveClass("z-terminal--green");
  });

  it("renders aria-label when label prop is provided", () => {
    render(<Terminal label="Quick start">npm install</Terminal>);
    expect(screen.getByLabelText("Quick start")).toBeInTheDocument();
  });

  it("renders as a <pre> element", () => {
    const { container } = render(<Terminal>cmd</Terminal>);
    expect(container.querySelector("pre")).toBeInTheDocument();
  });

  it("wraps content in a <code> element", () => {
    const { container } = render(<Terminal>cmd</Terminal>);
    expect(container.querySelector("code")).toBeInTheDocument();
  });

  it("sets data-command attribute for machine readability", () => {
    const { container } = render(<Terminal>curl -sSL https://usezombie.sh/install | bash</Terminal>);
    const pre = container.querySelector("pre");
    expect(pre).toHaveAttribute("data-command", "curl -sSL https://usezombie.sh/install | bash");
  });

  it("renders sr-only fallback label when label prop is not provided", () => {
    const { container } = render(<Terminal>cmd</Terminal>);
    expect(container.querySelector(".sr-only")).toHaveTextContent("Code block");
  });

  it("does not set data-command for non-string children", () => {
    const { container } = render(
      <Terminal>
        <span>cmd</span>
      </Terminal>,
    );
    expect(container.querySelector("pre")).not.toHaveAttribute("data-command");
  });
});

describe("Terminal — copyable", () => {
  it("does not render copy button by default", () => {
    render(<Terminal>npm install</Terminal>);
    expect(screen.queryByRole("button", { name: /copy/i })).toBeNull();
  });

  it("renders copy button when copyable=true", () => {
    render(<Terminal copyable>npm install</Terminal>);
    expect(screen.getByRole("button", { name: /copy command/i })).toBeInTheDocument();
  });

  it("button label is 'Copy command' initially", () => {
    render(<Terminal copyable>npm install</Terminal>);
    expect(screen.getByRole("button")).toHaveAttribute("aria-label", "Copy command");
  });

  it("calls clipboard.writeText with command on click", async () => {
    render(<Terminal copyable>curl test</Terminal>);
    await act(async () => {
      fireEvent.click(screen.getByRole("button"));
    });
    expect(mockWriteText).toHaveBeenCalledWith("curl test");
  });

  it("copies empty string when children is not plain text", async () => {
    render(
      <Terminal copyable>
        <span>cmd</span>
      </Terminal>,
    );
    await act(async () => {
      fireEvent.click(screen.getByRole("button"));
    });
    expect(mockWriteText).toHaveBeenCalledWith("");
  });

  it("button shows Copied! state after click", async () => {
    render(<Terminal copyable>curl test</Terminal>);
    await act(async () => {
      fireEvent.click(screen.getByRole("button"));
    });
    expect(screen.getByRole("button")).toHaveAttribute("aria-label", "Copied!");
    expect(screen.getByRole("button")).toHaveTextContent("✓ Copied");
  });

  it("button reverts to 'Copy command' after 2s", async () => {
    vi.useFakeTimers();
    render(<Terminal copyable>curl test</Terminal>);
    await act(async () => {
      fireEvent.click(screen.getByRole("button"));
    });
    expect(screen.getByRole("button")).toHaveAttribute("aria-label", "Copied!");
    await act(async () => {
      vi.advanceTimersByTime(2100);
    });
    expect(screen.getByRole("button")).toHaveAttribute("aria-label", "Copy command");
    vi.useRealTimers();
  });

  it("wraps pre + button in z-terminal-wrap div", () => {
    const { container } = render(<Terminal copyable>cmd</Terminal>);
    const wrap = container.querySelector(".z-terminal-wrap");
    expect(wrap).toBeInTheDocument();
    expect(wrap?.querySelector("pre")).toBeInTheDocument();
    expect(wrap?.querySelector("button")).toBeInTheDocument();
  });
});
