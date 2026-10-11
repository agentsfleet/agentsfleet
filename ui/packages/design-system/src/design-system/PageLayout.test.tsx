import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { PageLayout } from "./PageLayout";

describe("PageLayout", () => {
  it("merges caller class names and marks no full-height layout by default", () => {
    render(
      <PageLayout aria-label="Fleet workspace" className="custom-layout">
        Content
      </PageLayout>,
    );

    const layout = screen.getByLabelText("Fleet workspace");
    expect(layout).toHaveClass("custom-layout");
    expect(layout).not.toHaveAttribute("data-page-layout");
  });

  it("fills the available viewport when fullHeight is requested", () => {
    const ref = { current: null as HTMLDivElement | null };
    render(<PageLayout ref={ref} fullHeight>Content</PageLayout>);

    expect(ref.current).toBeInstanceOf(HTMLDivElement);
    expect(ref.current).toHaveClass("min-h-0", "flex-1");
    expect(ref.current).toHaveAttribute("data-page-layout", "full-height");
  });
});
