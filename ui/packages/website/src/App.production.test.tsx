import {afterAll, beforeAll, expect, it, vi} from "vitest";
import {render, screen} from "@testing-library/react";
import {createMemoryRouter, RouterProvider} from "react-router-dom";

beforeAll(() => vi.stubEnv("DEV", false));
afterAll(() => vi.unstubAllEnvs());

it("keeps the component gallery out of production routes", async () => {
  const {default: App} = await import("./App");
  const router = createMemoryRouter([{path: "*", element: <App />}], {initialEntries: ["/_design-system"]});
  render(<RouterProvider router={router} />);
  expect(await screen.findByRole("heading", {name: "This page isn’t here."}, {timeout: 10_000})).toBeInTheDocument();
  expect(screen.queryByRole("heading", {name: /design system gallery/i})).not.toBeInTheDocument();
  expect(screen.getByRole("main").closest(".website-palette")).toBeTruthy();
});
