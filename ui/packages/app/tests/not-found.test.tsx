import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import NotFound from "../app/not-found";
import DashboardNotFound from "../app/(dashboard)/not-found";

afterEach(cleanup);

it("gives an unavailable app page a heading and working dashboard recovery link", () => {
  render(<NotFound />);
  expect(screen.getByRole("heading", { name: "Page not found", level: 1 })).toBeTruthy();
  expect(screen.getByRole("link", { name: "Back to dashboard" }).getAttribute("href")).toBe("/");
});

it("uses the existing dashboard main landmark for an unavailable nested route", () => {
  const { container } = render(<main><DashboardNotFound /></main>);
  expect(container.querySelectorAll("main").length).toBe(1);
  expect(screen.getByRole("heading", { name: "Page not found", level: 1 })).toBeTruthy();
});
