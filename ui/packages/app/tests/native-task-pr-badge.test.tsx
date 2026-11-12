import type { TaskActivityHistory } from "@/lib/task-activity";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { FleetTaskProvider } from "@/components/domain/chat/FleetTaskProvider";
import { NativeTaskPrBadge } from "@/components/domain/chat/NativeTaskPrBadge";
import { TaskPanel } from "@/components/domain/chat/TaskPanel";
import { FleetWorkSidebar } from "@/components/domain/FleetWorkSidebar";
import { evt } from "./helpers/fleet-stream-fixtures";
import { detail, summary, TASK_IDS } from "./helpers/task-reads";
const SCOPE = {workspaceId: "workspace", fleetId: "fleet", subject: "indy", revision: 0};
afterEach(() => {cleanup(); vi.unstubAllGlobals();});
function activity(ids: (string | null)[]): TaskActivityHistory {
  return {items: ids.map(id => ({kind: "task_transfer_progress", progress: {activity_id: TASK_IDS[0], task_id: id, source_admission_id: null,
    operation: "capture", phase: "saved", provider: null, model: null, completed: 1, total: 1}})), truncated: false};
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

it("shows a standard PR badge before the mobile drawer opens, sharing reads and preserving draft on revocation", async () => {
  const fetcher = network();
  function View({subject = "indy"}: {subject?: string | null}) {
    return <FleetTaskProvider {...SCOPE} subject={subject}><textarea aria-label="Draft" defaultValue="Keep this direction" />
      <NativeTaskPrBadge activity={activity([TASK_IDS[0]])} />
      <FleetWorkSidebar><TaskPanel {...SCOPE} /></FleetWorkSidebar></FleetTaskProvider>;
  }
  const view = render(<View />);
  const badge = await screen.findByRole("link", {name: "Pull request #803 · a/b · Repair login redirect"});
  expect(badge.textContent).toBe("#803");
  expect(badge.classList.contains("ph-no-capture")).toBe(true);
  expect(badge.classList.contains("ph-no-record")).toBe(true);
  expect(badge.getAttribute("href")).toBe("https://github.com/a/b/pull/803");
  expect(badge.querySelector("svg")).toBeTruthy();
  expect(screen.getByRole("region", {name: "Current work"})).toBeTruthy();
  expect(fetcher).toHaveBeenCalledTimes(2);
  await screen.findByRole("region", {name: "Outputs"});
  expect(fetcher).toHaveBeenCalledTimes(2);
  const draft = screen.getByRole("textbox", {name: "Draft", hidden: true});
  view.rerender(<View subject={null} />);
  expect(screen.queryByRole("link", {name: /Pull request/})).toBeNull();
  expect(draft).toHaveProperty("value", "Keep this direction");
});

it("omits ambiguous, truncated, malformed, or unassociated activity instead of inferring a PR from text", async () => {
  network();
  const view = render(<FleetTaskProvider {...SCOPE}><NativeTaskPrBadge activity={activity([TASK_IDS[0]])} /></FleetTaskProvider>);
  await screen.findByRole("link");
  for (const value of [activity([TASK_IDS[0], TASK_IDS[1]]), activity([null]), {...activity([TASK_IDS[0]]), truncated: true}, "PR #803", activity([TASK_IDS[2]])]) {
    view.rerender(<FleetTaskProvider {...SCOPE}><NativeTaskPrBadge activity={value} /></FleetTaskProvider>);
    expect(screen.queryByRole("link")).toBeNull();
  }
  view.rerender(<FleetTaskProvider {...SCOPE}><NativeTaskPrBadge activity={activity([TASK_IDS[0], TASK_IDS[0], null])} /></FleetTaskProvider>);
  expect(screen.getByRole("link")).toBeTruthy();
});

it("withholds badge through an unauthorized list read and pending retry", async () => {
  const fetcher = network();
  const props = {children: <NativeTaskPrBadge activity={activity([TASK_IDS[0]])} />};
  const view = render(<FleetTaskProvider {...SCOPE} {...props} />);
  await screen.findByRole("link");
  fetcher.mockResolvedValue(new Response(null, {status: 403}));
  view.rerender(<FleetTaskProvider {...SCOPE} {...props} revision={1} />);
  await waitFor(() => expect(screen.queryByRole("link")).toBeNull());
  const pending = Promise.withResolvers<Response>(); fetcher.mockReturnValue(pending.promise);
  view.rerender(<FleetTaskProvider {...SCOPE} {...props} revision={2} />);
  expect(screen.queryByRole("link")).toBeNull();
  view.unmount();
  pending.resolve(Response.json(response()));
});

it("marks retained PR evidence stale during a failed refresh, then clears the badge when the native task is missing", async () => {
  const fetcher = network();
  const content = <><NativeTaskPrBadge activity={activity([TASK_IDS[0]])} /><TaskPanel {...SCOPE} /></>;
  const view = render(<FleetTaskProvider {...SCOPE}>{content}</FleetTaskProvider>);
  expect((await screen.findByRole("link", {name: "Pull request #803 · a/b · Repair login redirect"})).getAttribute("title")).toBe("Repair login redirect · a/b");
  fireEvent.click(screen.getByRole("button", {name: /^Tasks/}));
  fireEvent.click(screen.getByRole("button", {name: /PR #123 repair/}));
  let status = 503;
  fetcher.mockImplementation(async input => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return path.includes("?limit=") ? Response.json({items: [summary()], next_cursor: null}) : new Response(null, {status});
  });
  view.rerender(<FleetTaskProvider {...SCOPE} revision={1}>{content}</FleetTaskProvider>);
  await screen.findByText(/Last confirmed details are shown/);
  await waitFor(() => expect(screen.getByRole("link", {name: "Pull request #803 · a/b · Repair login redirect"}).getAttribute("title")).toBe("Repair login redirect · a/b · last confirmed"));
  status = 404;
  view.rerender(<FleetTaskProvider {...SCOPE} revision={2}>{content}</FleetTaskProvider>);
  await waitFor(() => expect(screen.queryByRole("link", {name: "Pull request #803 · a/b · Repair login redirect"})).toBeNull());
});


it("presents the explicitly associated native target as a result card with retained evidence", async () => {
  const fetcher = network();
  const content = <NativeTaskPrBadge activity={activity([TASK_IDS[0]])} appearance="card" />;
  const view = render(<FleetTaskProvider {...SCOPE}>{content}</FleetTaskProvider>);
  const card = await screen.findByRole("link", {name: "Pull request #803 · a/b · Repair login redirect"});
  expect(card.textContent).toContain("Repair login redirect");
  expect(card.textContent).toContain("Pull request #803 · a/b");
  expect(card.getAttribute("href")).toBe("https://github.com/a/b/pull/803");
  expect(fetcher).toHaveBeenCalledTimes(2);
  fetcher.mockImplementation(async input => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return path.includes("?limit=") ? Response.json({items: [summary()], next_cursor: null}) : new Response(null, {status: 503});
  });
  view.rerender(<FleetTaskProvider {...SCOPE} revision={1}>{content}</FleetTaskProvider>);
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
  const content = <NativeTaskPrBadge activity={activity([TASK_IDS[0]])} appearance="card" />;
  const view = render(<FleetTaskProvider {...SCOPE}>{content}</FleetTaskProvider>);
  expect(await screen.findByRole("status", {name: "Reading linked work"})).toBeTruthy();
  pending.resolve(Response.json(response()));
  expect(await screen.findByRole("link")).toHaveProperty("href", "https://github.com/a/b/pull/803");
  expect(screen.queryByRole("status", {name: "Reading linked work"})).toBeNull();
  fetcher.mockResolvedValue(new Response(null, {status: 403}));
  view.rerender(<FleetTaskProvider {...SCOPE} revision={1}>{content}</FleetTaskProvider>);
  await waitFor(() => expect(screen.queryByRole("link")).toBeNull());
  expect(screen.queryByRole("status", {name: "Reading linked work"})).toBeNull();
  const retry = Promise.withResolvers<Response>(); fetcher.mockReturnValue(retry.promise);
  view.rerender(<FleetTaskProvider {...SCOPE} revision={2}>{content}</FleetTaskProvider>);
  expect(screen.queryByRole("status", {name: "Reading linked work"})).toBeNull();
  view.rerender(<FleetTaskProvider {...SCOPE} subject={null}>{content}</FleetTaskProvider>);
  expect(screen.queryByRole("status", {name: "Reading linked work"})).toBeNull();
  retry.resolve(new Response(null, {status: 503}));
});

it("does not claim to read linked results without an identity-owned reader", () => {
  render(<NativeTaskPrBadge activity={activity([TASK_IDS[0]])} appearance="card" />);
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.queryByRole("link")).toBeNull();
});


it.each([503, 200])("stops reserving a result after the first list read fails or is invalid (%s)", async status => {
  const fetcher = vi.fn<typeof fetch>().mockImplementation(async () => status === 503 ? new Response(null, {status}) : Response.json({items: "invalid"}));
  vi.stubGlobal("fetch", fetcher);
  render(<FleetTaskProvider {...SCOPE}><NativeTaskPrBadge activity={activity([TASK_IDS[0]])} appearance="card" /></FleetTaskProvider>);
  await waitFor(() => expect(fetcher).toHaveBeenCalledTimes(1));
  await waitFor(() => expect(screen.queryByRole("status", {name: "Reading linked work"})).toBeNull());
  expect(screen.queryByRole("link")).toBeNull();
});

it("does not leave an older result loading outside the reader's bounded history window", async () => {
  const ids = Array.from({length: 17}, (_, index) => `019a42b0-0000-7000-8000-${String(index).padStart(12, "0")}`);
  const fetcher = vi.fn<typeof fetch>().mockImplementation(async input => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return Response.json(path.includes("?limit=") ? {items: [], next_cursor: null} : detail(summary(path.split("/").at(-1), "done")));
  });
  vi.stubGlobal("fetch", fetcher);
  render(<FleetTaskProvider {...SCOPE} events={[evt({taskActivity: activity(ids)})]}>
    <NativeTaskPrBadge activity={activity([ids[0]!])} appearance="card" />
  </FleetTaskProvider>);
  await waitFor(() => expect(fetcher).toHaveBeenCalledTimes(17));
  expect(fetcher.mock.calls.some(([input]) => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return path.endsWith(ids[0]!);
  })).toBe(false);
  expect(screen.queryByRole("status", {name: "Reading linked work"})).toBeNull();
  expect(screen.queryByRole("link")).toBeNull();
});
