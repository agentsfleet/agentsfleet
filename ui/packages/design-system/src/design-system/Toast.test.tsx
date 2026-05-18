import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
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
});
