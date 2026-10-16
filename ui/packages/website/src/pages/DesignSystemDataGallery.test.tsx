import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { AppPrimitivesGallery } from "./DesignSystemDataGallery";

afterEach(cleanup);

it("lets readers sort example fleets and page through their status and run counts", () => {
  render(<AppPrimitivesGallery />);
  expect(screen.getByRole("navigation", {name: "Example app sections"})).toBeInTheDocument();
  const table = screen.getByRole("table", {name: "Example fleets"});
  expect(within(table).getByText("Needs access")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", {name: /Next/}));
  expect(within(table).getByText("Dependency check")).toBeInTheDocument();
  expect(within(table).getByText("Paused")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", {name: /Previous/}));
  fireEvent.click(within(table).getByRole("button", {name: /Runs/}));
  expect(within(table).getAllByRole("row")[1]).toHaveTextContent("Support triage");
  fireEvent.click(within(table).getByRole("button", {name: /Fleet/}));
  expect(within(table).getAllByRole("row")[1]).toHaveTextContent("Checkout watch");
  fireEvent.click(within(table).getByRole("button", {name: /Status/}));
  expect(within(table).getAllByRole("row")[1]).toHaveTextContent("Active");
});
