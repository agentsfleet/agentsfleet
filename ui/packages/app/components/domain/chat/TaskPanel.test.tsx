import type { TaskSummary } from "@/lib/api/tasks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TaskPanel } from "./TaskPanel";
import { useFleetEventStream } from "../useFleetEventStream";
import { FRAME_KIND } from "@/lib/api/events-types";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { __resetRegistryForTests, streamWrapper } from "@/tests/helpers/fleet-streams";
import { TASK_IDS, detail, summary } from "@/tests/helpers/task-reads";

const SCOPE = {workspaceId: "workspace", fleetId: "linkwarden", subject: "indy", revision: 0};

beforeEach(() => {FakeEventSource.install(); __resetRegistryForTests();});
afterEach(() => {cleanup(); __resetRegistryForTests(); FakeEventSource.uninstall(); vi.unstubAllGlobals();});

function LiveWork() {
  const stream = useFleetEventStream(SCOPE.workspaceId, SCOPE.fleetId, []);
  return <TaskPanel {...SCOPE} revision={stream.tasksSeq} />;
}

function page(items: TaskSummary[]): Response {return Response.json({items, next_cursor: null});}

describe("current work in the existing fleet conversation", () => {
  it("reconciles work accepted between the initial read and the first ready stream", async () => {
    const network = vi.fn<typeof fetch>()
      .mockResolvedValueOnce(page([]))
      .mockResolvedValueOnce(page([summary()]));
    vi.stubGlobal("fetch", network);
    render(<LiveWork />, {wrapper: streamWrapper});
    await waitFor(() => expect(screen.queryByRole("region", {name: "Current work"})).toBeNull());
    expect(network).toHaveBeenCalledTimes(1);
    act(() => {
      FakeEventSource.instances[0]!.open();
      FakeEventSource.instances[0]!.emit({kind: FRAME_KIND.HELLO, fleet_ids: [SCOPE.fleetId]});
    });
    expect(await screen.findByText("PR #123 repair")).toBeTruthy();
    expect(screen.getByText("Waiting")).toBeTruthy();
    expect(network).toHaveBeenCalledTimes(2);
  });

  it("shows three repairs, lazy saved criteria, and confirmed progress after a streamed task change", async () => {
    const waiting = summary();
    const running = summary(TASK_IDS[1], "running");
    const queued = summary(TASK_IDS[2], "ready");
    const list = Promise.withResolvers<Response>();
    const saved = Promise.withResolvers<Response>();
    const changed = Promise.withResolvers<Response>();
    const restored = Promise.withResolvers<Response>();
    const network = vi.fn<typeof fetch>()
      .mockReturnValueOnce(list.promise)
      .mockReturnValueOnce(saved.promise)
      .mockResolvedValueOnce(new Response(null, {status: 503}))
      .mockReturnValueOnce(changed.promise)
      .mockReturnValueOnce(restored.promise)
      .mockResolvedValueOnce(new Response(null, {status: 403}))
      .mockResolvedValueOnce(page([]));
    vi.stubGlobal("fetch", network);
    render(<LiveWork />, {wrapper: streamWrapper});
    expect(screen.getByRole("status").textContent).toContain("Reading current work");
    expect(network).toHaveBeenCalledTimes(1);
    await act(async () => list.resolve(page([waiting, running, queued])));
    const panel = await screen.findByRole("region", {name: "Current work"});
    expect(panel.textContent).toContain("Current work · 3");
    expect(panel.textContent).toContain("1 waiting · 1 in progress · 1 queued");
    expect(screen.queryByText(/Does the regression pass/)).toBeNull();
    await userEvent.click(screen.getByRole("button", {name: /PR #123 repair/}));
    expect(screen.getByText("Reading saved progress…")).toBeTruthy();
    await act(async () => saved.resolve(Response.json(detail(waiting))));
    expect(await screen.findByText("What counts as done?")).toBeTruthy();
    expect(panel.textContent).toContain("Uses the success criteria saved in setup v2.");
    expect(panel.textContent).toContain("The exact proposed head passes the regression.");
    expect(panel.textContent).toContain("2 of 12 attempts used. No recovery point has been saved.");
    expect(network.mock.calls[1]?.[0]).toContain(`/tasks/${TASK_IDS[0]}`);

    act(() => {
      for (const malformed of [{task_id: "not-a-task", resource_version: 2}, {task_id: waiting.id, resource_version: -1}]) {
        FakeEventSource.instances[0]!.emitNamed(FRAME_KIND.TASK_CHANGED, JSON.stringify({kind: FRAME_KIND.TASK_CHANGED, ...malformed}));
      }
    });
    expect(network).toHaveBeenCalledTimes(2);
    expect(panel.textContent).toContain("Current work · 3");
    expect(screen.getByText("What counts as done?")).toBeTruthy();
    act(() => FakeEventSource.instances[0]!.emit({kind: FRAME_KIND.TASK_CHANGED, task_id: waiting.id, resource_version: 2}));
    expect(await screen.findByText("Couldn’t refresh. Last confirmed progress is shown.")).toBeTruthy();
    expect(screen.getByText("PR #123 repair")).toBeTruthy();
    expect(screen.getByText("What counts as done?")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", {name: "Refresh current work"}));
    expect(screen.getByText("Reading current work…")).toBeTruthy();
    expect(screen.getByText("PR #123 repair")).toBeTruthy();
    const resumed = summary(waiting.id, "running", 2);
    await act(async () => changed.resolve(page([resumed, summary(running.id, "waiting", 2), queued])));
    expect(await screen.findByText("1 in progress · 1 waiting · 1 queued")).toBeTruthy();
    expect(screen.getByText("Reading saved progress…")).toBeTruthy();
    // A newer list version does not replace confirmed detail until its own read answers.
    expect(screen.getByText(/No recovery point has been saved/)).toBeTruthy();
    await act(async () => restored.resolve(Response.json(detail(resumed, TASK_IDS[1]))));
    expect(await screen.findByText(/Saved recovery point available/)).toBeTruthy();

    await userEvent.click(screen.getByRole("button", {name: "Refresh current work"}));
    expect(await screen.findByText("Current work is unavailable. Refresh to try again.")).toBeTruthy();
    expect(screen.queryByText("PR #123 repair")).toBeNull();
    expect(screen.queryByText("What counts as done?")).toBeNull();
    await userEvent.click(screen.getByRole("button", {name: "Refresh current work"}));
    await waitFor(() => expect(screen.queryByRole("region", {name: "Current work"})).toBeNull());
    expect(network).toHaveBeenCalledTimes(7);
  });

  it("recovers unavailable details and retains their evidence through an unsuccessful refresh", async () => {
    const task = summary(TASK_IDS[0], "ready");
    const queued = summary(TASK_IDS[1], "ready");
    const network = vi.fn<typeof fetch>()
      .mockResolvedValueOnce(page([task, queued]))
      .mockResolvedValueOnce(new Response(null, {status: 503}))
      .mockResolvedValueOnce(Response.json(detail(task)))
      .mockResolvedValueOnce(page([summary(task.id, "blocked", 2), queued]))
      .mockResolvedValueOnce(new Response(null, {status: 503}))
      .mockResolvedValueOnce(page([summary(task.id, "blocked", 3), queued]))
      .mockResolvedValueOnce(new Response(null, {status: 404}));
    vi.stubGlobal("fetch", network);
    render(<TaskPanel {...SCOPE} />);
    expect(await screen.findByText("2 queued")).toBeTruthy();
    await userEvent.click(await screen.findByRole("button", {name: /PR #123 repair/}));
    expect(await screen.findByText("Saved progress is unavailable.")).toBeTruthy();
    expect(screen.queryByText("What counts as done?")).toBeNull();
    await userEvent.click(screen.getByRole("button", {name: "Retry details"}));
    expect(await screen.findByText("What counts as done?")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", {name: "Refresh current work"}));
    expect(await screen.findByText("Progress couldn’t be refreshed. Last confirmed details are shown.")).toBeTruthy();
    expect(screen.getByText("Needs attention")).toBeTruthy();
    expect(screen.getByText("Does the regression pass on the proposed head?", {exact: false})).toBeTruthy();
    await userEvent.click(screen.getByRole("button", {name: "Refresh current work"}));
    expect(await screen.findByText("Saved progress is unavailable.")).toBeTruthy();
    expect(screen.queryByText("What counts as done?")).toBeNull();
    expect(screen.getByText("Needs attention")).toBeTruthy();
    expect(network).toHaveBeenCalledTimes(7);
  });

  it("cancels old scoped reads and ignores their late replies after an account or fleet switch", async () => {
    const oldList = Promise.withResolvers<Response>();
    const oldDetail = Promise.withResolvers<Response>();
    const network = vi.fn<typeof fetch>()
      .mockReturnValueOnce(oldList.promise)
      .mockResolvedValueOnce(page([summary()]))
      .mockReturnValueOnce(oldDetail.promise)
      .mockResolvedValueOnce(page([summary(TASK_IDS[2], "ready")]));
    vi.stubGlobal("fetch", network);
    const view = render(<TaskPanel {...SCOPE} subject={null} />);
    expect(network).not.toHaveBeenCalled();
    view.rerender(<TaskPanel {...SCOPE} />);
    const listSignal = network.mock.calls[0]?.[1]?.signal;
    view.rerender(<TaskPanel {...SCOPE} fleetId="other-fleet" />);
    expect(listSignal?.aborted).toBe(true);
    await userEvent.click(await screen.findByRole("button", {name: /PR #123 repair/}));
    const detailSignal = network.mock.calls[2]?.[1]?.signal;
    view.rerender(<TaskPanel {...SCOPE} subject="another-owner" />);
    expect(detailSignal?.aborted).toBe(true);
    expect(await screen.findByText("PR #125 repair")).toBeTruthy();
    await act(async () => {oldList.resolve(page([summary(TASK_IDS[1], "running")])); oldDetail.resolve(Response.json(detail()));});
    expect(screen.queryByText("PR #124 repair")).toBeNull();
    expect(screen.queryByText("PR #123 repair")).toBeNull();
    expect(screen.queryByText("What counts as done?")).toBeNull();
    expect(network).toHaveBeenCalledTimes(4);
  });
});
