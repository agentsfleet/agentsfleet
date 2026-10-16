import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import Hero from "./Hero";
import { MemoryRouter } from "react-router-dom";
import { DEFAULT_JOB } from "../lib/example-jobs";
import { APP_SIGN_UP_URL } from "../config";

const INSTALL_COMMAND = "curl -fsSL https://agentsfleet.dev | bash";

it("keeps the incident request editable and opens signup without putting it in the URL", () => {
  const onRequest = vi.fn();
  const onJob = vi.fn();
  render(<MemoryRouter><Hero request={DEFAULT_JOB.request} onRequest={onRequest} job={DEFAULT_JOB} onJob={onJob} /></MemoryRouter>);
  const input = screen.getByRole("textbox", { name: "What should your fleet handle?" });
  fireEvent.change(input, { target: { value: "Investigate my build" } });
  expect(onRequest).toHaveBeenCalledWith("Investigate my build");
  const signup = screen.getByRole("link", { name: /Create your first fleet/ });
  expect(signup).toHaveAttribute("href", APP_SIGN_UP_URL);
  expect(signup).toHaveAttribute("target", "_blank");
  expect(signup).toHaveAttribute("rel", "noopener noreferrer");
  expect(screen.queryByRole("button", { name: /Review setup/ })).toBeNull();
  const incident = screen.getByRole("link", { name: /Investigate an incident.*Explore example run/ });
  expect(incident).toHaveAttribute("href", "/#how-it-works");
  fireEvent.click(incident);
  expect(onJob).toHaveBeenCalledWith(DEFAULT_JOB);
  expect(screen.getByText(INSTALL_COMMAND)).toBeInTheDocument();
});

it("replaces the composer with example content while retaining the installer", () => {
  render(<MemoryRouter><Hero request={DEFAULT_JOB.request} onRequest={vi.fn()} job={DEFAULT_JOB} onJob={vi.fn()}>
    <p>Example content</p>
  </Hero></MemoryRouter>);
  expect(screen.getByText("Example content")).toBeInTheDocument();
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.queryByRole("link", { name: /Create your first fleet/ })).toBeNull();
  expect(screen.getByText(INSTALL_COMMAND)).toBeInTheDocument();
  expect(screen.queryByText(/preview|release pending/i)).toBeNull();
});
