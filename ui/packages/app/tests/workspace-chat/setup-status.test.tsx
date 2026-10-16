import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { SetupProgress } from "@/components/domain/chat/SetupProgress";
import { SetupChecks } from "@/components/domain/chat/SetupChecks";
import { SetupReviewDetails } from "@/components/domain/chat/SetupReviewDetails";
import { initialDefinition, safeActionHref } from "@/components/domain/chat/chat-defaults";
import { FLEET, REVIEW, WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { NANOS_PER_USD } from "@/lib/types";

afterEach(cleanup);

it("shows all readiness outcomes and refuses external or backslash recovery destinations", () => {
  const checks = ["ready", "blocked", "missing", "pending"].map((state, index) => ({kind: "access", subject: `Check ${index}`, state,
    next_action: {kind: "workspace_connections", href: ["/settings", "//outside.test", "https://outside.test", "/\\outside.test"][index]}}));
  render(<SetupChecks checks={checks} label="Readiness" />);
  for (const state of ["ready", "blocked", "missing", "pending"]) expect(screen.getByText(state)).toBeTruthy();
  expect(screen.getAllByRole("link")).toHaveLength(1);
  expect(screen.getByRole("link").getAttribute("href")).toBe("/settings");
  expect(safeActionHref(undefined)).toBeUndefined();
});

it("keeps Continue available until a requested first run is durably accepted", () => {
  const proceed = vi.fn();
  const fleet = {...FLEET, status: "active", start: {kind: "run_now" as const, state: "pending" as const}};
  const view = render(<SetupProgress fleet={fleet} workspace={WORKSPACE} onContinue={proceed} busy />);
  expect(screen.getByRole("button", {name: "Continue setup"})).toHaveProperty("disabled", true);
  view.rerender(<SetupProgress fleet={fleet} workspace={WORKSPACE} onContinue={proceed} busy={false} />);
  fireEvent.click(screen.getByRole("button", {name: "Continue setup"}));
  expect(proceed).toHaveBeenCalledOnce();
  view.rerender(<SetupProgress fleet={{...fleet, start: {kind: "run_now", state: "accepted"}}} workspace={WORKSPACE} onContinue={proceed} busy={false} />);
  expect(screen.queryByRole("button", {name: "Continue setup"})).toBeNull();
  expect(screen.getByText(/Accepted work can still be running/)).toBeTruthy();
});

it("shows unresolved defaults and the exact source and fleet revisions under review", () => {
  const definition = initialDefinition();
  expect(definition.config["x-agentsfleet"]).toMatchObject({tools: [], budget: {daily_nanos: String(NANOS_PER_USD)}});
  render(<SetupReviewDetails review={{...REVIEW, effective: {...REVIEW.effective, definition, runtime_model_ref: null, source_revision: "source-2", target_setup_revision: "3"}}} />);
  expect(screen.getByText("Account default is not resolved")).toBeTruthy();
  expect(screen.getByText("source-2")).toBeTruthy();
  expect(screen.getByText("Fleet setup revision")).toBeTruthy();
  expect(screen.getByText("Add instructions before saving or publishing.")).toBeTruthy();
});
