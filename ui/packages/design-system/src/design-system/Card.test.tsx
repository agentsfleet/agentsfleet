import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from "./Card";

describe("Card", () => {
  it("renders children inside an <article> by default", () => {
    const { container } = render(
      <Card>
        <p>Hello card</p>
      </Card>,
    );
    expect(container.firstChild?.nodeName).toBe("ARTICLE");
    expect(screen.getByText("Hello card")).toBeInTheDocument();
  });

  it("applies base card utilities", () => {
    const { container } = render(<Card>Content</Card>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("rounded-lg");
    expect(cls).toContain("border-border");
    expect(cls).toContain("bg-card");
  });

  it("does not render the popular badge when not featured", () => {
    render(<Card>Content</Card>);
    expect(screen.queryByText("Popular")).not.toBeInTheDocument();
  });

  it("renders the default 'Popular' badge when featured", () => {
    render(<Card featured>Featured</Card>);
    expect(screen.getByText("Popular")).toBeInTheDocument();
  });

  it("overrides the badge label via badgeLabel prop", () => {
    render(<Card featured badgeLabel="Best value">Featured</Card>);
    expect(screen.getByText("Best value")).toBeInTheDocument();
    expect(screen.queryByText("Popular")).not.toBeInTheDocument();
  });

  it("applies featured styles when featured", () => {
    const { container } = render(<Card featured>X</Card>);
    expect((container.firstChild as HTMLElement).className).toContain("border-primary");
  });

  it("merges a custom className", () => {
    const { container } = render(<Card className="my-card">Content</Card>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("my-card");
    expect(cls).toContain("rounded-lg");
  });

  it("forwards arbitrary props (data-testid)", () => {
    render(<Card data-testid="my-card">Content</Card>);
    expect(screen.getByTestId("my-card")).toBeInTheDocument();
  });

  it("asChild renders the provided child as root (preserves Card classes)", () => {
    const { container } = render(
      <Card asChild>
        <section data-testid="as-section">content</section>
      </Card>,
    );
    const section = container.firstChild as HTMLElement;
    expect(section.tagName).toBe("SECTION");
    expect(section.className).toContain("rounded-lg");
  });

  it("forwards ref to the underlying element", () => {
    const ref = { current: null as HTMLElement | null };
    render(<Card ref={ref}>X</Card>);
    expect(ref.current?.tagName).toBe("ARTICLE");
  });

  it("SSR renders <article> markup with card classes", () => {
    const html = renderToStaticMarkup(<Card>Hi</Card>);
    expect(html).toMatch(/^<article /);
    expect(html).toContain("rounded-lg");
  });

  it("composes sub-parts in a dashboard layout", () => {
    render(
      <Card>
        <CardHeader>
          <CardTitle>Title</CardTitle>
          <CardDescription>Desc</CardDescription>
        </CardHeader>
        <CardContent>Body</CardContent>
        <CardFooter>Footer</CardFooter>
      </Card>,
    );
    expect(screen.getByText("Title")).toBeInTheDocument();
    expect(screen.getByText("Desc")).toBeInTheDocument();
    expect(screen.getByText("Body")).toBeInTheDocument();
    expect(screen.getByText("Footer")).toBeInTheDocument();
  });
});
