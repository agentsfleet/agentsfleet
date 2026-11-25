import type { DurableTaskDetail } from "@/lib/api/tasks";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DURABLE_TASK_IDS, detail, summary } from "@/tests/helpers/task-reads";
import { DurableTaskPanel } from "./TaskPanel";
import { DurableTaskStatements } from "./TaskStatements";

vi.mock("@/lib/actions/fleet-approvals", () => ({listApprovalsAction: async () => ({ok: false, status: 503, error: "Unavailable"})}));
const TAG = `"${"a".repeat(64)}"`;
const TASK_PATH = `/live/v1/workspaces/workspace/fleets/fleet/durable_tasks/${DURABLE_TASK_IDS[0]}`;
const MINE = "Does PR 88's head pass the null-cart regression? 🦉";
type Call = {method: string; path: string; body?: unknown; headers?: HeadersInit};
let calls: Call[];
let patch: () => Response | Promise<Response>;
let generation: number;

beforeEach(() => {
  calls = []; generation = 1;
  patch = () => Response.json(detail(), {headers: {etag: TAG}});
  vi.stubGlobal("fetch", vi.fn(async (path: string, init: RequestInit = {}) => {
    const method = init.method ?? "GET";
    calls.push({method, path, body: typeof init.body === "string" ? JSON.parse(init.body) : undefined, headers: init.headers});
    const task = summary();
    if (method === "PATCH") return patch();
    if (path.includes("?limit=")) return Response.json({items: [{...task, generation}], next_cursor: null});
    const read = detail({...task, generation});
    return Response.json(read, {headers: {etag: TAG}});
  }));
});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

async function openTask() {
  render(<DurableTaskPanel workspaceId="workspace" fleetId="fleet" subject="person" revision={0} />);
  fireEvent.click(await screen.findByRole("button", {name: /^Tasks/}));
  fireEvent.click(await screen.findByRole("button", {name: /PR #123 repair/}));
  await waitFor(() => expect(screen.getByRole("button", {name: "Edit statements"})).toHaveProperty("disabled", false));
}
function edit(text: string) {
  fireEvent.click(screen.getByRole("button", {name: "Edit statements"}));
  fireEvent.change(screen.getByRole("textbox", {name: "Statement 1"}), {target: {value: text}});
}

it("a stale statement edit reloads the task", async () => {
  patch = () => {generation = 2; return Response.json({user_message: "The task changed."}, {status: 412});};
  await openTask();
  const before = calls.filter(call => call.method === "GET").length;
  edit(MINE);
  fireEvent.click(screen.getByRole("button", {name: "Save statements"}));
  expect(await screen.findByText(/This task changed while you were editing/)).toBeTruthy();
  const patches = calls.filter(call => call.method === "PATCH");
  expect(patches).toHaveLength(1);
  expect(patches[0]).toMatchObject({path: TASK_PATH, headers: expect.objectContaining({"If-Match": TAG})});
  expect(patches[0]?.body).toEqual({questions: [{...detail().context.questions[0], question: {...detail().context.questions[0]?.question, instructions: MINE}}], expected_generation: 1});
  await waitFor(() => expect(calls.filter(call => call.method === "GET" && call.path.includes("?limit=")).length).toBeGreaterThan(1));
  await waitFor(() => expect(calls.filter(call => call.method === "GET").length).toBeGreaterThanOrEqual(before + 2));
  expect(screen.getByRole<HTMLTextAreaElement>("textbox", {name: "Statement 1"}).value).toBe(MINE);
  patch = () => Response.json(detail(), {headers: {etag: TAG}});
  await waitFor(() => expect(screen.getByRole("button", {name: "Save statements"})).toHaveProperty("disabled", false));
  fireEvent.click(screen.getByRole("button", {name: "Save statements"}));
  await waitFor(() => expect(calls.filter(call => call.method === "PATCH")).toHaveLength(2));
  await waitFor(() => expect(screen.queryByRole("form", {name: "Edit completion statements"})).toBeNull());
  expect(calls.filter(call => call.method === "PATCH").map(call => (call.body as {expected_generation: number}).expected_generation)).toEqual([1, 2]);
});

describe("a task's completion statements", () => {
  const ready = {...detail(), etag: TAG};
  function statements(over: Partial<{detail: DurableTaskDetail; readOnly: boolean; enabled: boolean}> = {}) {
    const onChanged = vi.fn();
    const view = render(<DurableTaskStatements workspaceId="workspace" fleetId="fleet" detail={over.detail ?? ready} enabled={over.enabled ?? true}
      readOnly={over.readOnly ?? false} onChanged={onChanged} />);
    return {onChanged, view};
  }

  it("reads each statement with what yes and no mean", () => {
    statements();
    const [statement] = detail().context.questions;
    expect(screen.getByText(`1. ${statement?.question.instructions}`)).toBeTruthy();
    expect(screen.getByText(statement?.question.criteria.true ?? "")).toBeTruthy();
    expect(screen.getByText(statement?.question.criteria.false ?? "")).toBeTruthy();
    expect(screen.queryByText(/setup v/)).toBeNull();
  });

  it.each([
    {name: "finished", over: {detail: {...ready, context: {...ready.context, state: "done" as const}}}},
    {name: "archived", over: {readOnly: true}},
  ])("offers no edit on a $name task", ({over}) => {
    statements(over);
    expect(screen.queryByRole("button", {name: "Edit statements"})).toBeNull();
  });

  it("waits for a confirmed read before editing, and saves no blank statement", () => {
    const {view} = statements({enabled: false});
    expect(screen.getByRole("button", {name: "Edit statements"})).toHaveProperty("disabled", true);
    view.rerender(<DurableTaskStatements workspaceId="workspace" fleetId="fleet" detail={ready} enabled readOnly={false} onChanged={vi.fn()} />);
    edit("   ");
    expect(document.activeElement).toBe(screen.getByRole("textbox", {name: "Statement 1"}));
    expect(screen.getByRole("button", {name: "Save statements"})).toHaveProperty("disabled", true);
    fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
    expect(screen.queryByRole("form", {name: "Edit completion statements"})).toBeNull();
    expect(document.activeElement).toBe(screen.getByRole("button", {name: "Edit statements"}));
    expect(calls).toEqual([]);
  });

  it("rewords only the statement a person edited", async () => {
    const [first] = ready.context.questions;
    if (!first) throw new Error("the fixture task carries a statement");
    const second = {...first, name: "green", question: {...first.question, instructions: "Are PR 88's checks green?"}};
    const {onChanged} = statements({detail: {...ready, context: {...ready.context, questions: [first, second]}}});
    fireEvent.click(screen.getByRole("button", {name: "Edit statements"}));
    fireEvent.change(screen.getByRole("textbox", {name: "Statement 2"}), {target: {value: MINE}});
    fireEvent.click(screen.getByRole("button", {name: "Save statements"}));
    await waitFor(() => expect(onChanged).toHaveBeenCalledOnce());
    expect(document.activeElement).toBe(screen.getByRole("button", {name: "Edit statements"}));
    expect(calls.find(call => call.method === "PATCH")?.body).toEqual({questions: [first, {...second, question: {...second.question, instructions: MINE}}], expected_generation: 1});
  });

  it("keeps the words when a confirmation cannot be read", async () => {
    patch = () => Response.json({wrong: true}, {headers: {etag: TAG}});
    statements();
    edit(MINE);
    fireEvent.click(screen.getByRole("button", {name: "Save statements"}));
    expect(await screen.findByText("The change could not be confirmed. Refresh progress before trying again.")).toBeTruthy();
    expect(screen.getByRole<HTMLTextAreaElement>("textbox", {name: "Statement 1"}).value).toBe(MINE);
  });

  it.each([
    [403, "Only a signed-in person can change these statements."],
    [429, "The task reached a limit. Its statements are unchanged."],
    [503, "The change could not be confirmed. Refresh progress before trying again."],
  ])("keeps the words and says why a %i was refused", async (status, sentence) => {
    patch = () => new Response(null, {status});
    const {onChanged} = statements();
    edit(MINE);
    fireEvent.click(screen.getByRole("button", {name: "Save statements"}));
    expect(await screen.findByText(new RegExp(sentence.slice(0, 30)))).toBeTruthy();
    expect(screen.getByRole<HTMLTextAreaElement>("textbox", {name: "Statement 1"}).value).toBe(MINE);
    expect(onChanged).not.toHaveBeenCalled();
  });

  it("sends one edit for two submissions, and drops an answer that lands after it closed", async () => {
    const held = Promise.withResolvers<Response>();
    patch = () => held.promise;
    const {onChanged, view} = statements();
    edit(MINE);
    const form = screen.getByRole("form", {name: "Edit completion statements"});
    fireEvent.submit(form); fireEvent.submit(form);
    expect(await within(form).findByText("Saving statements…")).toBeTruthy();
    expect(calls.filter(call => call.method === "PATCH")).toHaveLength(1);
    view.unmount();
    await act(async () => held.resolve(Response.json(detail(), {headers: {etag: TAG}})));
    expect(onChanged).not.toHaveBeenCalled();
  });
});
