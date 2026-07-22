import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { Pagination } from "./Pagination";

describe("Pagination — cursor-backed feeds", () => {
  // A keyset feed cannot count itself, so `total` is absent and `hasNext` is
  // the only thing that knows whether the feed continues.
  it("advances while the caller reports another page", () => {
    const onPageChange = vi.fn();
    render(<Pagination kind="page" page={2} pageSize={25} hasNext onPageChange={onPageChange} />);
    expect(screen.getByText("Page 2")).toBeInTheDocument();
    const next = screen.getByRole("button", { name: "Next page" });
    expect(next).not.toBeDisabled();
    fireEvent.click(next);
    expect(onPageChange).toHaveBeenCalledWith(3);
  });

  it("stops at the end of the feed instead of offering an empty page", () => {
    // Without `hasNext` an unknown total leaves Next live forever, which is
    // exactly the dead click this flag exists to prevent.
    render(<Pagination kind="page" page={3} pageSize={25} hasNext={false} onPageChange={() => {}} />);
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
  });

  it("lets an explicit hasNext override what the total implies", () => {
    render(
      <Pagination kind="page" page={1} pageSize={25} total={100} hasNext={false} onPageChange={() => {}} />,
    );
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
  });

  it("uses flex-wrap so buttons reflow on narrow viewports", () => {
    render(<Pagination kind="page" page={1} pageSize={25} hasNext onPageChange={() => {}} />);
    expect(screen.getByTestId("pagination-page").className).toContain("flex-wrap");
  });
});

describe("Pagination (page variant)", () => {
  it("renders Page X of Y with aria-live=polite and correct button states", () => {
    const onPageChange = vi.fn();
    render(<Pagination kind="page" page={2} pageSize={20} total={87} onPageChange={onPageChange} />);
    expect(screen.getByText("Page 2 of 5 · 87 items")).toBeInTheDocument();
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

  it("shows visible progress while a numeric page is loading", () => {
    render(<Pagination kind="page" page={2} pageSize={20} total={87} onPageChange={() => {}} isLoading />);
    expect(screen.getByText("Page 2 of 5 · 87 items")).toBeInTheDocument();
    expect(screen.getByText("Loading…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Previous page" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
  });

  it("SSR renders with role=navigation", () => {
    const html = renderToStaticMarkup(
      <Pagination kind="page" page={1} pageSize={25} hasNext onPageChange={() => {}} />,
    );
    expect(html).toContain('role="navigation"');
  });
});
