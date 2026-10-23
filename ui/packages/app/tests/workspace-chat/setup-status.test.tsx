import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { SetupProgress } from "@/components/domain/chat/SetupProgress";
import { SetupChecks } from "@/components/domain/chat/SetupChecks";
import { FleetSetupDetails } from "@/components/domain/chat/FleetSetupDetails";
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

it("hides infrastructure subjects while retaining resource names and unknown checks", () => {
  const modelSubject = "internal-model-ref";
  const resource = "github";
  const unknownKind = "unrecognized-check";
  const unknownSubject = "resource-needs-attention";
  render(<SetupChecks label="Readiness" checks={[
    {kind: "model", subject: modelSubject, state: "ready"},
    {kind: "credentials", subject: resource, state: "blocked"},
    {kind: unknownKind, subject: unknownSubject, state: "missing"},
  ]} />);
  expect(screen.getByText("Model availability")).toBeTruthy();
  expect(screen.queryByText(modelSubject)).toBeNull();
  expect(screen.getByText("Connected account")).toBeTruthy();
  expect(screen.getByText(resource)).toBeTruthy();
  expect(screen.getByText(unknownKind)).toBeTruthy();
  expect(screen.getByText(unknownSubject)).toBeTruthy();
});

it("keeps Continue available until a requested first run is durably accepted", () => {
  const proceed = vi.fn();
  const fleet = {...FLEET, status: "active", start: {kind: "run_now" as const, state: "pending" as const}};
  const view = render(<SetupProgress fleet={fleet} workspace={WORKSPACE} onContinue={proceed} onRetrySchedule={vi.fn()} busy />);
  expect(screen.getByRole("button", {name: "Continue setup"})).toHaveProperty("disabled", true);
  view.rerender(<SetupProgress fleet={fleet} workspace={WORKSPACE} onContinue={proceed} onRetrySchedule={vi.fn()} busy={false} />);
  fireEvent.click(screen.getByRole("button", {name: "Continue setup"}));
  expect(proceed).toHaveBeenCalledOnce();
  view.rerender(<SetupProgress fleet={{...fleet, start: {kind: "run_now", state: "accepted"}}} workspace={WORKSPACE} onContinue={proceed} onRetrySchedule={vi.fn()} busy={false} />);
  expect(screen.queryByRole("button", {name: "Continue setup"})).toBeNull();
  expect(screen.getByText(/Accepted work can still be running/)).toBeTruthy();
});

it("omits saved readiness disclosure when the fleet has no recorded checks", () => {
  render(<SetupProgress fleet={{...FLEET, setup: {state: "ready", checks: []}}} workspace={WORKSPACE} onContinue={vi.fn()} onRetrySchedule={vi.fn()} busy={false} />);
  expect(screen.queryByRole("button", {name: "What needs attention"})).toBeNull();
  expect(screen.getByRole("link", {name: "Open fleet"})).toBeTruthy();
});

it("calls an active fleet ready only when every schedule is registered, and retries the rest", () => {
  const retry = vi.fn();
  const synced = {schedule_id: "sched-ok", fleet_id: FLEET.id, sync: "synced"};
  const failed = {schedule_id: "sched-bad", fleet_id: FLEET.id, sync: "failed", last_error: "scheduler refused"};
  const unconfirmed = {schedule_id: "sched-wait", fleet_id: FLEET.id, sync: "syncing", last_error: null};
  const active = {...FLEET, status: "active"};
  const view = render(<SetupProgress fleet={{...active, schedules: [synced]}} workspace={WORKSPACE} onContinue={vi.fn()} onRetrySchedule={retry} busy={false} />);
  expect(screen.getByText(`${FLEET.name} is ready`)).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Retry registration"})).toBeNull();
  view.rerender(<SetupProgress fleet={{...active, schedules: [synced, failed, unconfirmed]}} workspace={WORKSPACE} onContinue={vi.fn()} onRetrySchedule={retry} busy={false} />);
  expect(screen.getByText(`${FLEET.name} is active, but a schedule is not registered`)).toBeTruthy();
  expect(screen.getByText("Schedule sched-bad is saved but not registered, so it will not fire (scheduler refused).")).toBeTruthy();
  expect(screen.getByText(/Schedule sched-wait is saved but its registration was not confirmed/)).toBeTruthy();
  const buttons = screen.getAllByRole("button", {name: "Retry registration"});
  expect(buttons).toHaveLength(2);
  fireEvent.click(buttons[0]!);
  expect(retry).toHaveBeenCalledWith(failed);
  view.rerender(<SetupProgress fleet={{...active, schedules: [{...failed, last_error: null}]}} workspace={WORKSPACE} onContinue={vi.fn()} onRetrySchedule={retry} busy={false} />);
  expect(screen.getByText("Schedule sched-bad is saved but not registered, so it will not fire.")).toBeTruthy();
});

it("shows unresolved defaults and the exact source and fleet revisions under review", () => {
  const definition = initialDefinition();
  expect(definition.config["x-agentsfleet"]).toMatchObject({tools: [], budget: {daily_nanos: String(NANOS_PER_USD)}});
  render(<FleetSetupDetails review={{...REVIEW, effective: {...REVIEW.effective, definition, runtime_model_ref: null, source_revision: "source-2", target_setup_revision: "3"}}} />);
  expect(screen.getByText("Account default is not resolved")).toBeTruthy();
  expect(screen.getByText("source-2")).toBeTruthy();
  expect(screen.getByText("Fleet setup revision")).toBeTruthy();
  expect(screen.getByText("Add instructions before saving or publishing.")).toBeTruthy();
});
