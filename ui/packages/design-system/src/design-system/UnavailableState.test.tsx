import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Button } from "./Button";
import { UnavailableState } from "./UnavailableState";

afterEach(cleanup);

it("announces an unavailable read with its explanation and caller-owned recovery", () => {
  const retry = vi.fn();
  const view = render(<UnavailableState title="Library unavailable" description="Please retry shortly."
    actions={<Button onClick={retry}>Retry</Button>} />);
  expect(screen.getByRole("alert")).toHaveTextContent("Library unavailable");
  expect(screen.getByRole("alert")).toHaveTextContent("Please retry shortly.");
  fireEvent.click(screen.getByRole("button", {name: "Retry"}));
  expect(retry).toHaveBeenCalledOnce();
  view.rerender(<UnavailableState title="Library unavailable" description="Ask an administrator for access." />);
  expect(screen.queryByRole("button")).toBeNull();
  expect(screen.getByRole("alert")).toHaveTextContent("Ask an administrator for access.");
});
