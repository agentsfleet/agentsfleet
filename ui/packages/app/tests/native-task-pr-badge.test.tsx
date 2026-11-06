import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { FleetTaskProvider } from "@/components/domain/chat/FleetTaskProvider";
import { NativeTaskPrBadge } from "@/components/domain/chat/NativeTaskPrBadge";
import { TaskPanel } from "@/components/domain/chat/TaskPanel";
import { FleetWorkSidebar } from "@/components/domain/FleetWorkSidebar";
import { detail, summary, TASK_IDS } from "./helpers/task-reads";
import type { TaskActivityHistory } from "@/lib/task-activity";
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
  expect(badge.querySelector("svg.lucide-git-pull-request")).toBeTruthy();
  expect(screen.queryByRole("region", {name: "Current work"})).toBeNull();
  expect(fetcher).toHaveBeenCalledTimes(2);
  fireEvent.click(screen.getByRole("button", {name: "Work context"}));
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
  expect((await screen.findByRole("link", {name: /^Pull request #803/})).getAttribute("title")).toBe("Repair login redirect · a/b");
  fireEvent.click(screen.getByRole("button", {name: /PR #123 repair/}));
  let status = 503;
  fetcher.mockImplementation(async input => {
    const path = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    return path.includes("?limit=") ? Response.json({items: [summary()], next_cursor: null}) : new Response(null, {status});
  });
  view.rerender(<FleetTaskProvider {...SCOPE} revision={1}>{content}</FleetTaskProvider>);
  await screen.findByText(/Last confirmed details are shown/);
  await waitFor(() => expect(screen.getByRole("link", {name: /^Pull request #803/}).getAttribute("title")).toBe("Repair login redirect · a/b · last confirmed"));
  status = 404;
  view.rerender(<FleetTaskProvider {...SCOPE} revision={2}>{content}</FleetTaskProvider>);
  await waitFor(() => expect(screen.queryByRole("link", {name: /^Pull request #803/})).toBeNull());
});
