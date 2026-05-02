import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Alert, alertVariants, type AlertProps } from "./Alert";

const variants: Array<{ variant: AlertProps["variant"]; tokens: string[] }> = [
  { variant: "info", tokens: ["text-info", "bg-info/10", "border-info/40"] },
  { variant: "success", tokens: ["text-success", "bg-success/10", "border-success/40"] },
  { variant: "warning", tokens: ["text-warning", "bg-warning/10", "border-warning/40"] },
  {
    variant: "destructive",
    tokens: ["text-destructive", "bg-destructive/10", "border-destructive/40"],
  },
];

describe("Alert", () => {
  it("renders each variant with the matching theme-token classes", () => {
    for (const { variant, tokens } of variants) {
      const { container, unmount } = render(
        <Alert variant={variant}>{variant}</Alert>,
      );
      const cls = (container.firstChild as HTMLElement).className;
      for (const tok of tokens) expect(cls).toContain(tok);
      unmount();
    }
  });

  it("defaults role by severity (alert for destructive/warning, status otherwise)", () => {
    render(<Alert variant="destructive">d</Alert>);
    expect(screen.getByText("d").closest("[role]")?.getAttribute("role")).toBe("alert");

    render(<Alert variant="warning">w</Alert>);
    expect(screen.getByText("w").closest("[role]")?.getAttribute("role")).toBe("alert");

    render(<Alert variant="info">i</Alert>);
    expect(screen.getByText("i").closest("[role]")?.getAttribute("role")).toBe("status");

    render(<Alert variant="success">s</Alert>);
    expect(screen.getByText("s").closest("[role]")?.getAttribute("role")).toBe("status");
  });

  it("defaults to info variant + status role when variant prop is omitted", () => {
    const { container } = render(<Alert>neutral</Alert>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toContain("text-info");
    expect(el.getAttribute("role")).toBe("status");
  });

  it("respects an explicit role override", () => {
    const { container } = render(
      <Alert variant="info" role="alert">
        loud
      </Alert>,
    );
    expect((container.firstChild as HTMLElement).getAttribute("role")).toBe("alert");
  });

  it("fires onDismiss exactly once when the dismiss button is clicked", () => {
    const onDismiss = vi.fn();
    render(
      <Alert variant="info" onDismiss={onDismiss}>
        body
      </Alert>,
    );
    const btn = screen.getByRole("button", { name: /dismiss/i });
    fireEvent.click(btn);
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("renders no dismiss button when onDismiss is not provided", () => {
    render(<Alert variant="info">no dismiss</Alert>);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("forwards props to the child element when asChild is set", () => {
    const { container } = render(
      <Alert variant="info" asChild>
        <a href="/x">go</a>
      </Alert>,
    );
    const el = container.firstChild as HTMLElement;
    expect(el.tagName).toBe("A");
    expect(el.getAttribute("role")).toBe("status");
    expect(el.className).toContain("text-info");
  });

  it("merges a custom className", () => {
    const { container } = render(
      <Alert variant="info" className="custom-x">
        x
      </Alert>,
    );
    expect((container.firstChild as HTMLElement).className).toContain("custom-x");
  });

  it("alertVariants returns the destructive token string", () => {
    expect(alertVariants({ variant: "destructive" })).toContain("text-destructive");
  });
});
