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

  it("preserves divided utility classes when asChild swaps the host element", () => {
    // Bug this catches: a previous refactor passed variant classes through Slot
    // but dropped the `divided` class set, silently breaking the borderless
    // separator UX on every divided + asChild call site.
    const { container } = render(
      <List divided asChild>
        <menu data-testid="m">
          <ListItem>one</ListItem>
          <ListItem>two</ListItem>
        </menu>
      </List>,
    );
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("border-b");
    expect(cls).toContain("border-border");
  });

  it("renders an empty <ul> for a children-less List (no crash, no spurious markup)", () => {
    const { container } = render(<List>{[]}</List>);
    const el = container.firstChild as HTMLElement;
    expect(el.tagName).toBe("UL");
    expect(el.children.length).toBe(0);
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

  describe("ListItem bullet variants", () => {
    it("no bullet prop → renders <li> with only the user's content (no leading span)", () => {
      const { container } = render(
        <ul>
          <ListItem>just text</ListItem>
        </ul>,
      );
      const li = container.querySelector("li");
      expect(li).not.toBeNull();
      // Guard: no aria-hidden artefact, no leading span injected.
      expect(li?.querySelector("span[aria-hidden=\"true\"]")).toBeNull();
      expect(li?.textContent).toBe("just text");
    });

    it("bullet='arrow' → renders the ↳ glyph in --text-subtle with aria-hidden", () => {
      const { container } = render(
        <ul>
          <ListItem bullet="arrow">stage one</ListItem>
        </ul>,
      );
      const li = container.querySelector("li");
      const span = li?.querySelector("span[aria-hidden=\"true\"]");
      expect(span).not.toBeNull();
      expect(span?.textContent).toBe("↳");
      // Bullet is muted (text-subtle) and has the canonical 8px gap (mr-md).
      expect(span?.className).toContain("text-text-subtle");
      expect(span?.className).toContain("mr-md");
      // User content is preserved alongside the bullet span.
      expect(li?.textContent).toContain("stage one");
    });

    it("bullet='dot' → renders the · glyph in --text-subtle with aria-hidden", () => {
      const { container } = render(
        <ul>
          <ListItem bullet="dot">priority support</ListItem>
        </ul>,
      );
      const li = container.querySelector("li");
      const span = li?.querySelector("span[aria-hidden=\"true\"]");
      expect(span).not.toBeNull();
      expect(span?.textContent).toBe("·");
      expect(span?.className).toContain("text-text-subtle");
      expect(span?.className).toContain("mr-md");
      expect(li?.textContent).toContain("priority support");
    });

    it("merges consumer className alongside the bullet span", () => {
      // Catches a regression where the bullet branch dropped className.
      const { container } = render(
        <ul>
          <ListItem bullet="arrow" className="font-mono text-text-muted">x</ListItem>
        </ul>,
      );
      const li = container.querySelector("li");
      expect(li?.className).toContain("font-mono");
      expect(li?.className).toContain("text-text-muted");
      expect(li?.querySelector("span[aria-hidden=\"true\"]")?.textContent).toBe("↳");
    });
  });
});
