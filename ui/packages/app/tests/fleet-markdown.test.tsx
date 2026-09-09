import React from "react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { FleetMarkdown } from "@/components/domain/FleetMarkdown";

afterEach(cleanup);

describe("FleetMarkdown", () => {
  // The bug the operator reported: a model wrote a bullet list and the chat
  // printed the asterisks, then folded every newline into one paragraph.
  it("renders a bullet list as a list, not as literal asterisks", () => {
    const { container } = render(
      <FleetMarkdown>{"To get going I need:\n\n- **A repo** name\n- The diff"}</FleetMarkdown>,
    );
    expect(container.querySelectorAll("li")).toHaveLength(2);
    expect(screen.getByText("A repo").tagName).toBe("STRONG");
    expect(container.textContent).not.toContain("**");
  });

  it("renders numbered lists, headings and inline code", () => {
    const { container } = render(
      <FleetMarkdown>{"### Steps\n\n1. run `make lint-all`\n2. push"}</FleetMarkdown>,
    );
    expect(container.querySelector("ol")).toBeTruthy();
    expect(screen.getByText("make lint-all").tagName).toBe("CODE");
    expect(screen.getByText("Steps")).toBeTruthy();
  });

  it("renders a fenced block without also chipping the code inside it", () => {
    const { container } = render(
      <FleetMarkdown>{"```sh\nmake test-unit-all\n```"}</FleetMarkdown>,
    );
    const code = container.querySelector("pre code");
    expect(code).toBeTruthy();
    // Inside a fence the chip styling would double the background.
    expect(code?.className ?? "").toBe("");
  });

  // GFM, via remark-gfm: a model that writes a table gets a table.
  it("renders a GFM table inside its own scroll container", () => {
    const { container } = render(
      <FleetMarkdown>{"| a | b |\n| - | - |\n| 1 | 2 |"}</FleetMarkdown>,
    );
    expect(container.querySelector("table")).toBeTruthy();
    expect(container.querySelectorAll("th")).toHaveLength(2);
    expect(container.querySelector("div.overflow-x-auto")).toBeTruthy();
  });

  // The author is a model, so every link leaves the dashboard and cannot reach
  // back through window.opener.
  it("opens links away from the dashboard", () => {
    render(<FleetMarkdown>{"[docs](https://example.com)"}</FleetMarkdown>);
    const link = screen.getByRole("link", { name: "docs" });
    expect(link.getAttribute("target")).toBe("_blank");
    expect(link.getAttribute("rel")).toContain("noopener");
  });

  // react-markdown never parses raw HTML, which is the posture worth having
  // when the author is a model.
  it("does not execute HTML a model writes", () => {
    const { container } = render(
      <FleetMarkdown>{'<img src=x onerror="alert(1)">hello'}</FleetMarkdown>,
    );
    expect(container.querySelector("img")).toBeNull();
    expect(container.textContent).toContain("hello");
  });
});
