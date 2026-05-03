import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { List, ListItem, listVariants } from "./List";

describe("List", () => {
  it("defaults to <ul> with disc + pl-5", () => {
    const { container } = render(
      <List>
        <ListItem>one</ListItem>
        <ListItem>two</ListItem>
      </List>,
    );
    const el = container.firstChild as HTMLElement;
    expect(el.tagName).toBe("UL");
    expect(el.className).toContain("list-disc");
    expect(el.className).toContain("pl-5");
  });

  it("renders <ol> with list-decimal when variant='ordered'", () => {
    const { container } = render(
      <List variant="ordered">
        <ListItem>one</ListItem>
      </List>,
    );
    const el = container.firstChild as HTMLElement;
    expect(el.tagName).toBe("OL");
    expect(el.className).toContain("list-decimal");
  });

  it("renders <ul> with list-none + pl-0 for variant='plain'", () => {
    const { container } = render(
      <List variant="plain">
        <ListItem>one</ListItem>
      </List>,
    );
    const el = container.firstChild as HTMLElement;
    expect(el.tagName).toBe("UL");
    expect(el.className).toContain("list-none");
    expect(el.className).toContain("pl-0");
  });

  it("applies a border on non-last items when divided is set", () => {
    const { container } = render(
      <List divided>
        <ListItem>one</ListItem>
        <ListItem>two</ListItem>
      </List>,
    );
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("border-b");
    expect(cls).toContain("border-border");
  });

  it("renders the host element passed via asChild and keeps variant classes", () => {
    const { container } = render(
      <List asChild>
        <menu data-testid="m">
          <ListItem>one</ListItem>
        </menu>
      </List>,
    );
    const el = container.firstChild as HTMLElement;
    expect(el.tagName).toBe("MENU");
    expect(el.className).toContain("list-disc");
  });

  it("merges a custom className", () => {
    const { container } = render(
      <List className="custom-x">
        <ListItem>x</ListItem>
      </List>,
    );
    expect((container.firstChild as HTMLElement).className).toContain("custom-x");
  });

  it("listVariants returns the plain token string", () => {
    expect(listVariants({ variant: "plain" })).toContain("list-none");
  });

  it("defaults role='list' on the host (Safari VoiceOver list-none guard)", () => {
    const { container: ul } = render(<List variant="plain"><ListItem>x</ListItem></List>);
    expect((ul.firstChild as HTMLElement).getAttribute("role")).toBe("list");

    const { container: ol } = render(<List variant="ordered"><ListItem>x</ListItem></List>);
    expect((ol.firstChild as HTMLElement).getAttribute("role")).toBe("list");
  });

  it("respects an explicit role override", () => {
    const { container } = render(
      <List role="navigation"><ListItem>x</ListItem></List>,
    );
    expect((container.firstChild as HTMLElement).getAttribute("role")).toBe("navigation");
  });

  it("ListItem renders <li> and forwards className", () => {
    const { container } = render(
      <ul>
        <ListItem className="custom-li">x</ListItem>
      </ul>,
    );
    const li = container.querySelector("li");
    expect(li?.className).toContain("custom-li");
  });
});
