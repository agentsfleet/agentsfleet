import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import Fleets from "./Fleets";

it("shows how to create a fleet and keep talking to it in the terminal", () => {
  render(<Fleets />);
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Start a fleet from your terminal");
  const block = screen.getByLabelText("Fleet commands");
  expect(block).toHaveTextContent("agentsfleet login");
  expect(block).toHaveTextContent('agentsfleet fleet create "Check my latest deploy and report what you find." --name deploy-watch');
  expect(block).toHaveTextContent("agentsfleet fleet steer @deploy-watch");
  expect(screen.getByRole("link", { name: "Read the docs" })).toHaveAttribute("href", "https://docs.agentsfleet.net");
});

it("shows the first message and follow-ups as API calls", () => {
  render(<Fleets />);
  const api = screen.getByRole("region", { name: "Work with the API" });
  expect(api).toHaveTextContent("POST /v1/workspaces/{workspace_id}/fleets");
  expect(api).toHaveTextContent("Send the fleet’s first message with an operation ID.");
  expect(api).toHaveTextContent("POST /v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages");
  expect(api).toHaveTextContent("GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/turns/stream");
  expect(screen.getByRole("link", { name: "OpenAPI JSON" })).toHaveAttribute("href", "/openapi.json");
});

it("keeps the software metadata", () => {
  const { container } = render(<Fleets />);
  const script = container.querySelector('script[type="application/ld+json"]');
  const data = JSON.parse(script?.textContent ?? "null");
  expect(data).toMatchObject({ "@type": "SoftwareApplication", name: "agentsfleet" });
});
