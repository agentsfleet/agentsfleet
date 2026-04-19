import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { PageHeader } from "./PageHeader";

describe("PageHeader", () => {
  it("renders as a <div> with children", () => {
    const { container, getByText } = render(
      <PageHeader>
        <span>Title</span>
        <span>Actions</span>
      </PageHeader>,
    );
    expect(container.firstChild?.nodeName).toBe("DIV");
    expect(getByText("Title")).toBeInTheDocument();
    expect(getByText("Actions")).toBeInTheDocument();
  });

  it("applies base layout utilities (flex + space-between)", () => {
    const { container } = render(<PageHeader />);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("flex");
    expect(cls).toContain("items-center");
    expect(cls).toContain("justify-between");
  });

  it("merges consumer className without dropping base utilities", () => {
    const { container } = render(<PageHeader className="pt-10" />);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("pt-10");
    expect(cls).toContain("flex");
  });

  it("forwards native div props", () => {
    const { container } = render(<PageHeader data-testid="hdr" role="banner" />);
    const el = container.firstChild as HTMLElement;
    expect(el.getAttribute("data-testid")).toBe("hdr");
    expect(el.getAttribute("role")).toBe("banner");
  });
});
