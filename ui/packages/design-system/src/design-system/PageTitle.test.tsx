import { createRef } from "react";
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { PageTitle } from "./PageTitle";

const HEADING = "heading";
const CUSTOM_SIZE = "text-2xl";

describe("PageTitle", () => {
  it.each([
    {size: undefined, title: "Workspaces", typography: "text-display-md"},
    {size: "chat" as const, title: "Fleet chat", typography: "text-chat-title"},
  ])("renders $title as an accessible heading and forwards its reference", ({size, title, typography}) => {
    const ref = createRef<HTMLHeadingElement>();
    render(<PageTitle size={size} ref={ref}>{title}</PageTitle>);
    const heading = screen.getByRole(HEADING, {level: 1, name: title});
    expect(heading.nodeName).toBe("H1");
    expect(ref.current).toBe(heading);
    expect(heading).toHaveClass(typography);
  });

  it("merges consumer className without losing base utilities", () => {
    render(<PageTitle className={CUSTOM_SIZE}>T</PageTitle>);
    const cls = screen.getByRole(HEADING).className;
    expect(cls).toContain(CUSTOM_SIZE);
    expect(cls).toContain("font-semibold");
  });
});
