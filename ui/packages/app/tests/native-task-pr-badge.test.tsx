import type { DurableTaskActivityHistory } from "@/lib/task-activity";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import * as durableTaskActivity from "@/lib/task-activity";
import { FleetDurableTaskProvider } from "@/components/domain/tasks/FleetTaskProvider";
import { NativeDurableTaskPrBadge } from "@/components/domain/tasks/NativeTaskPrBadge";
import { DurableTaskPanel } from "@/components/domain/tasks/TaskPanel";
import { FleetWorkSidebar } from "@/components/domain/shared/summary/FleetWorkSidebar";
import { evt } from "./helpers/fleet-stream-fixtures";
import { detail, summary, DURABLE_TASK_IDS } from "./helpers/task-reads";
const SCOPE = {workspaceId: "workspace", fleetId: "fleet", subject: "indy", revision: 0};
afterEach(() => {cleanup(); vi.unstubAllGlobals();});
function activity(ids: (string | null)[]): DurableTaskActivityHistory {
  return {items: ids.map(id => ({kind: "durable_task_transfer_progress", progress: {activity_id: DURABLE_TASK_IDS[0], durable_task_id: id, source_turn_id: null,
    operation: "capture", phase: "saved", provider: null, model: null, completed: 1, total: 1}}))};
}
function response() {
  const saved = detail();
  return {...saved, title: "Repair login redirect", context: {...saved.context, target: {kind: "existing_pr", repository: "a/b", number: 803,
    branch: "repair", head: "a".repeat(40), tree: "b".repeat(40), url: "https://github.com/a/b/pull/803", workflows: []}}};
}
function network() {
  const fetcher = vi.fn<typeof fetch>().mockImplementation(async input => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return Response.json(path.includes("?limit=") ? {items: [summary()], next_cursor: null} : response());
  });
  vi.stubGlobal("fetch", fetcher);
  return fetcher;
}

it("shows the linked PR card before the mobile drawer opens, sharing reads and preserving draft on revocation", async () => {
  const fetcher = network();
  function View({subject = "indy"}: {subject?: string | null}) {
    return <FleetDurableTaskProvider {...SCOPE} subject={subject}><textarea aria-label="Draft" defaultValue="Keep this direction" />
      <NativeDurableTaskPrBadge activity={activity([DURABLE_TASK_IDS[0]])} />
      <FleetWorkSidebar><DurableTaskPanel {...SCOPE} /></FleetWorkSidebar></FleetDurableTaskProvider>;
  }
  const view = render(<View />);
  const badge = await screen.findByRole("link", {name: "Pull request #803 · a/b · Repair login redirect"});
  expect(badge.textContent).toContain("Pull request #803 · a/b");
  expect(badge.classList.contains("ph-no-capture")).toBe(true);
  expect(badge.classList.contains("ph-no-record")).toBe(true);
  expect(badge.getAttribute("href")).toBe("https://github.com/a/b/pull/803");
  expect(badge.querySelector("svg")).toBeTruthy();
  expect(screen.getByRole("region", {name: "Current work"})).toBeTruthy();
  expect(fetcher).toHaveBeenCalledTimes(2);
  await screen.findByRole("region", {name: "Pull requests"});
  expect(fetcher).toHaveBeenCalledTimes(2);
  const draft = screen.getByRole("textbox", {name: "Draft", hidden: true});
  view.rerender(<View subject={null} />);
  expect(screen.queryByRole("link", {name: /Pull request/})).toBeNull();
  expect(draft).toHaveProperty("value", "Keep this direction");
});

it("omits ambiguous, malformed, or unassociated activity instead of inferring a PR from text", async () => {
  network();
  const view = render(<FleetDurableTaskProvider {...SCOPE}><NativeDurableTaskPrBadge activity={activity([DURABLE_TASK_IDS[0]])} /></FleetDurableTaskProvider>);
  await screen.findByRole("link");
  for (const value of [activity([DURABLE_TASK_IDS[0], DURABLE_TASK_IDS[1]]), activity([null]), "PR #803", activity([DURABLE_TASK_IDS[2]])]) {
    view.rerender(<FleetDurableTaskProvider {...SCOPE}><NativeDurableTaskPrBadge activity={value} /></FleetDurableTaskProvider>);
    expect(screen.queryByRole("link")).toBeNull();
  }
  view.rerender(<FleetDurableTaskProvider {...SCOPE}><NativeDurableTaskPrBadge activity={activity([DURABLE_TASK_IDS[0], DURABLE_TASK_IDS[0], null])} /></FleetDurableTaskProvider>);
  expect(screen.getByRole("link")).toBeTruthy();
});

it("withholds the card through an unauthorized list read and pending retry", async () => {
  const fetcher = network();
  const props = {children: <NativeDurableTaskPrBadge activity={activity([DURABLE_TASK_IDS[0]])} />};
  const view = render(<FleetDurableTaskProvider {...SCOPE} {...props} />);
  await screen.findByRole("link");
  fetcher.mockResolvedValue(new Response(null, {status: 403}));
  view.rerender(<FleetDurableTaskProvider {...SCOPE} {...props} revision={1} />);
  await waitFor(() => expect(screen.queryByRole("link")).toBeNull());
  const pending = Promise.withResolvers<Response>(); fetcher.mockReturnValue(pending.promise);
  view.rerender(<FleetDurableTaskProvider {...SCOPE} {...props} revision={2} />);
  expect(screen.queryByRole("link")).toBeNull();
  view.unmount();
  pending.resolve(Response.json(response()));
});

it("marks retained PR evidence stale during a failed refresh, then clears the card when the native task is missing", async () => {
  const fetcher = network();
  const content = <><NativeDurableTaskPrBadge activity={activity([DURABLE_TASK_IDS[0]])} /><DurableTaskPanel {...SCOPE} /></>;
  const view = render(<FleetDurableTaskProvider {...SCOPE}>{content}</FleetDurableTaskProvider>);
  expect((await screen.findByRole("link", {name: "Pull request #803 · a/b · Repair login redirect"})).textContent).not.toContain("last confirmed");
  let status = 503;
  fetcher.mockImplementation(async input => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return path.includes("?limit=") ? Response.json({items: [summary()], next_cursor: null}) : new Response(null, {status});
  });
  view.rerender(<FleetDurableTaskProvider {...SCOPE} revision={1}>{content}</FleetDurableTaskProvider>);
  await screen.findByText(/Last confirmed details are shown/);
  await waitFor(() => expect(screen.getByRole("link", {name: "Pull request #803 · a/b · Repair login redirect"}).textContent).toContain("last confirmed"));
  status = 404;
  view.rerender(<FleetDurableTaskProvider {...SCOPE} revision={2}>{content}</FleetDurableTaskProvider>);
  await waitFor(() => expect(screen.queryByRole("link", {name: "Pull request #803 · a/b · Repair login redirect"})).toBeNull());
});


it("presents the explicitly associated native target as a result card with retained evidence", async () => {
  const fetcher = network();
  const content = <NativeDurableTaskPrBadge activity={activity([DURABLE_TASK_IDS[0]])} />;
  const view = render(<FleetDurableTaskProvider {...SCOPE}>{content}</FleetDurableTaskProvider>);
  const card = await screen.findByRole("link", {name: "Pull request #803 · a/b · Repair login redirect"});
  expect(card.textContent).toContain("Repair login redirect");
  expect(card.textContent).toContain("Pull request #803 · a/b");
  expect(card.getAttribute("href")).toBe("https://github.com/a/b/pull/803");
  expect(fetcher).toHaveBeenCalledTimes(2);
  fetcher.mockImplementation(async input => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return path.includes("?limit=") ? Response.json({items: [summary()], next_cursor: null}) : new Response(null, {status: 503});
  });
  view.rerender(<FleetDurableTaskProvider {...SCOPE} revision={1}>{content}</FleetDurableTaskProvider>);
  await screen.findByText(/last confirmed/);
  expect(card.getAttribute("href")).toBe("https://github.com/a/b/pull/803");
});

it("reserves linked-result space while the native detail is pending and withholds it after access loss", async () => {
  const pending = Promise.withResolvers<Response>();
  const fetcher = network();
  fetcher.mockImplementation(async input => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return path.includes("?limit=") ? Response.json({items: [summary()], next_cursor: null}) : pending.promise;
  });
  const content = <NativeDurableTaskPrBadge activity={activity([DURABLE_TASK_IDS[0]])} />;
  const view = render(<FleetDurableTaskProvider {...SCOPE}>{content}</FleetDurableTaskProvider>);
  expect(await screen.findByRole("status", {name: "Reading linked work"})).toBeTruthy();
  pending.resolve(Response.json(response()));
  expect(await screen.findByRole("link")).toHaveProperty("href", "https://github.com/a/b/pull/803");
  expect(screen.queryByRole("status", {name: "Reading linked work"})).toBeNull();
  fetcher.mockResolvedValue(new Response(null, {status: 403}));
  view.rerender(<FleetDurableTaskProvider {...SCOPE} revision={1}>{content}</FleetDurableTaskProvider>);
  await waitFor(() => expect(screen.queryByRole("link")).toBeNull());
  expect(screen.queryByRole("status", {name: "Reading linked work"})).toBeNull();
  const retry = Promise.withResolvers<Response>(); fetcher.mockReturnValue(retry.promise);
  view.rerender(<FleetDurableTaskProvider {...SCOPE} revision={2}>{content}</FleetDurableTaskProvider>);
  expect(screen.queryByRole("status", {name: "Reading linked work"})).toBeNull();
  view.rerender(<FleetDurableTaskProvider {...SCOPE} subject={null}>{content}</FleetDurableTaskProvider>);
  expect(screen.queryByRole("status", {name: "Reading linked work"})).toBeNull();
  retry.resolve(new Response(null, {status: 503}));
});

it("does not claim to read linked results without an identity-owned reader", () => {
  render(<NativeDurableTaskPrBadge activity={activity([DURABLE_TASK_IDS[0]])} />);
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.queryByRole("link")).toBeNull();
});


it.each([503, 200])("stops reserving a result after the first list read fails or is invalid (%s)", async status => {
  const fetcher = vi.fn<typeof fetch>().mockImplementation(async () => status === 503 ? new Response(null, {status}) : Response.json({items: "invalid"}));
  vi.stubGlobal("fetch", fetcher);
  render(<FleetDurableTaskProvider {...SCOPE}><NativeDurableTaskPrBadge activity={activity([DURABLE_TASK_IDS[0]])} /></FleetDurableTaskProvider>);
  await waitFor(() => expect(fetcher).toHaveBeenCalledTimes(1));
  await waitFor(() => expect(screen.queryByRole("status", {name: "Reading linked work"})).toBeNull());
  expect(screen.queryByRole("link")).toBeNull();
});

it("does not re-render a stable linked-work card for stream turns it does not read", async () => {
  network();
  const reads = vi.spyOn(durableTaskActivity, "readDurableTaskActivityHistory");
  // One element, reused: only a context change can re-render the card.
  const card = <NativeDurableTaskPrBadge activity={activity([DURABLE_TASK_IDS[0]])} />;
  const turns = (count: number) => Array.from({length: count}, (_, index) => evt({id: `text-${index}`, reply: `chunk ${index}`}));
  const view = render(<FleetDurableTaskProvider {...SCOPE} turns={turns(0)}>{card}</FleetDurableTaskProvider>);
  await screen.findByRole("link");
  const settled = reads.mock.calls.length;
  for (let count = 1; count <= 20; count++) view.rerender(<FleetDurableTaskProvider {...SCOPE} turns={turns(count)}>{card}</FleetDurableTaskProvider>);
  expect(reads.mock.calls.length).toBe(settled);
  expect(screen.getByRole("link")).toBeTruthy();
  reads.mockRestore();
});
