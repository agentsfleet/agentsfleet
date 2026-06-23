import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { PageHeader, PageDescription } from "./PageHeader";

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

  it("test_pageheader_description_below: description renders after the title in DOM order", () => {
    const { getByText } = render(
      <PageHeader description="Manage credits and usage.">
        <span>Billing</span>
      </PageHeader>,
    );
    const title = getByText("Billing");
    const desc = getByText("Manage credits and usage.");
    expect(desc).toBeInTheDocument();
    // description follows the title in document order (it renders below it)
    expect(title.compareDocumentPosition(desc) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(desc.tagName).toBe("P");
    expect(desc.className).toContain("text-muted-foreground");
  });

  it("pins an actions cluster to the top-right alongside the title column", () => {
    const { getByText } = render(
      <PageHeader description="d" actions={<button type="button">Add credential</button>}>
        <span>Credentials</span>
      </PageHeader>,
    );
    expect(getByText("Add credential").tagName).toBe("BUTTON");
    expect(getByText("Credentials")).toBeInTheDocument();
    expect(getByText("d").tagName).toBe("P");
  });

  it("PageDescription renders a muted <p> and merges className", () => {
    const { getByText } = render(<PageDescription className="extra-cls">Helper</PageDescription>);
    const p = getByText("Helper");
    expect(p.tagName).toBe("P");
    expect(p.className).toContain("text-muted-foreground");
    expect(p.className).toContain("extra-cls");
  });

  it("renders the structured header with actions but no description (no <p>)", () => {
    const { container, getByText } = render(
      <PageHeader actions={<button type="button">Edit</button>}>
        <span>Settings</span>
      </PageHeader>,
    );
    expect(getByText("Settings")).toBeInTheDocument();
    expect(getByText("Edit").tagName).toBe("BUTTON");
    // description omitted → no PageDescription <p> rendered
    expect(container.querySelector("p")).toBeNull();
  });
});
