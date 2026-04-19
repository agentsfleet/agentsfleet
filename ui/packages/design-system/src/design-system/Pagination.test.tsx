import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { Pagination } from "./Pagination";

describe("Pagination (cursor variant)", () => {
  it("enables Load more when a cursor is present", () => {
    const onNext = vi.fn();
    render(<Pagination kind="cursor" nextCursor="abc123" onNext={onNext} />);
    const nav = screen.getByTestId("pagination-cursor");
    expect(nav).toHaveAttribute("role", "navigation");
    expect(nav).toHaveAttribute("aria-label", "Feed pagination");
    const btn = screen.getByRole("button", { name: "Load more items" });
    expect(btn).not.toBeDisabled();
    fireEvent.click(btn);
    expect(onNext).toHaveBeenCalledWith("abc123");
  });

  it("shows End of feed and disables the button when nextCursor is null", () => {
    render(<Pagination kind="cursor" nextCursor={null} onNext={() => {}} />);
    expect(screen.getByText("End of feed")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Load more items" })).toBeDisabled();
  });

  it("shows Loading… while fetching", () => {
    render(<Pagination kind="cursor" nextCursor="abc" onNext={() => {}} isLoading />);
    expect(screen.getByText(/Loading/)).toBeInTheDocument();
  });

  it("uses flex-wrap so buttons reflow on narrow viewports", () => {
    render(<Pagination kind="cursor" nextCursor="abc" onNext={() => {}} />);
    expect(screen.getByTestId("pagination-cursor").className).toContain("flex-wrap");
  });
});

describe("Pagination (page variant)", () => {
  it("renders Page X of Y with aria-live=polite and correct button states", () => {
    const onPageChange = vi.fn();
    render(<Pagination kind="page" page={2} pageSize={20} total={87} onPageChange={onPageChange} />);
    expect(screen.getByText("Page 2 of 5")).toBeInTheDocument();
    const prev = screen.getByRole("button", { name: "Previous page" });
    const next = screen.getByRole("button", { name: "Next page" });
    expect(prev).not.toBeDisabled();
    expect(next).not.toBeDisabled();
    fireEvent.click(prev);
    expect(onPageChange).toHaveBeenCalledWith(1);
    fireEvent.click(next);
    expect(onPageChange).toHaveBeenCalledWith(3);
  });

  it("disables Previous on page 1", () => {
    render(<Pagination kind="page" page={1} pageSize={20} total={87} onPageChange={() => {}} />);
    expect(screen.getByRole("button", { name: "Previous page" })).toBeDisabled();
  });

  it("falls back to Page N when total is unknown", () => {
    render(<Pagination kind="page" page={4} pageSize={20} onPageChange={() => {}} />);
    expect(screen.getByText("Page 4")).toBeInTheDocument();
    expect(screen.queryByText(/Page\s+\d+\s+of\s+\d+/)).not.toBeInTheDocument();
  });

  it("SSR renders with role=navigation", () => {
    const html = renderToStaticMarkup(
      <Pagination kind="cursor" nextCursor="abc" onNext={() => {}} />,
    );
    expect(html).toContain('role="navigation"');
  });
});
