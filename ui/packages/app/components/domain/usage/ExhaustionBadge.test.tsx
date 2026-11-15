import {cleanup, render, screen} from "@testing-library/react";
import {renderToString} from "react-dom/server";
import {afterEach, expect, it} from "vitest";
import ExhaustionBadge from "./ExhaustionBadge";

afterEach(cleanup);

it("keeps a server-rendered exhaustion notice free of the server's clock", () => {
  const server = renderToString(<ExhaustionBadge exhaustedAt="2026-11-14T13:30:00Z" />);
  expect(server).toContain('title="Balance exhausted"');
  expect(server).not.toContain("13:30");
  render(<ExhaustionBadge exhaustedAt="2026-11-14T13:30:00Z" />);
  expect(screen.getByRole("status").title).toBe("Exhausted since Nov 14, 2026, 01:30 PM");
});

it("keeps an unknown exhaustion time unknown", () => {
  render(<ExhaustionBadge exhaustedAt={null} />);
  expect(screen.getByRole("status").title).toBe("Balance exhausted");
});
