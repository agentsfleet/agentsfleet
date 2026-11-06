import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { dialogElement, event } from "./helpers/event-details-dialog-fixtures";
import { serveDetail } from "./helpers/event-details-dialog-served";

vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", async () =>
  (await import("./helpers/event-details-dialog-served")).fleetActionsMock());
afterEach(cleanup);

it.each(["detail", "supplied"] as const)("uses the fresher %s state without showing a stale reply", async (newer) => {
  const working = event({status: "received", updated_at: "2026-11-04T09:00:00Z", response_text: "Earlier reply that must stay hidden."});
  const failed = event({status: "fleet_error", updated_at: "2026-11-04T09:00:00.100Z",
    failure_label: "oom_kill", failure_detail: "Memory cap reached", response_text: null});
  const supplied = newer === "supplied" ? failed : working;
  serveDetail(newer === "detail" ? failed : working);
  await act(async () => {render(dialogElement(supplied)); await Promise.resolve();});
  expect(await screen.findByLabelText("Failed event")).toBeTruthy();
  expect(await screen.findByText("Ran out of memory — Memory cap reached")).toBeTruthy();
  expect(screen.queryByLabelText("Event in progress")).toBeNull();
  expect(screen.queryByText("Earlier reply that must stay hidden.")).toBeNull();
  expect(screen.queryByText("No result recorded")).toBeNull();
  expect(screen.getByRole("dialog", {name: "Event details"})).toBeTruthy();
});

it("keeps a newer supplied completion unconfirmed until its result is read", async () => {
  const supplied = event({status: "processed", updated_at: "2026-11-04T09:00:00.100Z", failure_label: null});
  serveDetail(event({status: "received", updated_at: "2026-11-04T09:00:00Z", response_text: "Earlier reply that must stay hidden."}));
  await act(async () => {render(dialogElement(supplied)); await Promise.resolve();});
  expect(await screen.findByText("Latest result not yet available")).toBeTruthy();
  expect(screen.queryByText("No result recorded")).toBeNull();
  expect(screen.queryByText("Earlier reply that must stay hidden.")).toBeNull();
  expect(screen.getByLabelText("Successful event")).toBeTruthy();
});
