import type { AppendMessage } from "@assistant-ui/react";
import { act, cleanup, fireEvent, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@agentsfleet/design-system";
import { FLEET_DETAIL, FLEET_ID, MODEL, TURN_ID, WORKSPACE, response } from "@/tests/helpers/fleet-fixtures";
import { SEND_LABEL, UUID_V7 } from "@/tests/fleet-thread/steer-copy";
import starters from "@/public/fleet-starters.json" with { type: "json" };
import { NewFleetChat } from "./NewFleetChat";
import { useFleetCreation } from "./useFleetCreation";

const navigation = vi.hoisted(() => ({replace: vi.fn()}));
vi.mock("next/navigation", () => ({useRouter: () => navigation, usePathname: () => "/w/workspace/fleets/new"}));
const mint = vi.hoisted(() => ({fail: false}));
vi.mock("@/lib/streaming/operation-id", async original => {
  const actual = await original<typeof import("@/lib/streaming/operation-id")>();
  return {...actual, mintOperationId: () => {if (mint.fail) throw new actual.MintUnavailable(); return actual.mintOperationId();}};
});

const CREATE = `/live/v1/workspaces/${WORKSPACE}/fleets`;
const MESSAGE = "Watch checkout errors and tell me when the null-cart bug comes back 🦉";
const CREATED = () => response({fleet: FLEET_DETAIL, accepted: {turn_id: TURN_ID, disposition: "queued"}, replayed: false}, 201);
const REFUSED = () => response({user_message: "No model is available to this workspace. Choose one in Models.", error_code: "UZ-SETUP-008"}, 409);
let answers: (() => Response | Promise<Response>)[];
let posts: {message: string; operation_id: string; model_ref?: string}[];

beforeEach(() => {
  answers = []; posts = []; mint.fail = false; navigation.replace.mockReset();
  vi.stubGlobal("fetch", vi.fn(async (path: string, init: RequestInit) => {
    if (path !== CREATE || init.method !== "POST") return response({}, 599);
    if (typeof init.body === "string") posts.push(JSON.parse(init.body));
    const next = answers.shift();
    return next ? next() : CREATED();
  }));
});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

function mount() {return render(<TooltipProvider><NewFleetChat workspace={WORKSPACE} models={[MODEL]} /></TooltipProvider>);}
function composer(): HTMLTextAreaElement {return screen.getByRole("textbox", {name: "What would you like your fleet to do?"});}
async function send(text: string) {
  fireEvent.change(composer(), {target: {value: text}});
  await act(async () => {fireEvent.click(screen.getByRole("button", {name: SEND_LABEL}));});
}

describe("New fleet", () => {
  it("sends a first message as one create and moves to the fleet it made", async () => {
    mount();
    expect(screen.getByRole("heading", {name: "New fleet"})).toBeTruthy();
    await send(MESSAGE);
    await waitFor(() => expect(navigation.replace).toHaveBeenCalledWith(`/w/${WORKSPACE}/fleets/${FLEET_ID}`));
    expect(posts).toEqual([{message: MESSAGE, operation_id: expect.stringMatching(UUID_V7)}]);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("a refused create returns its text to the composer", async () => {
    answers.push(REFUSED);
    mount();
    await send(MESSAGE);
    const reason = await screen.findByText("No model is available to this workspace. Choose one in Models.");
    expect(reason.closest("[role=alert]")).toBeTruthy();
    expect(composer().value).toBe(MESSAGE);
    const notices = screen.getByRole("list", {name: "Unsent messages"});
    expect(within(notices).getByText("Message not sent.")).toBeTruthy();
    expect(navigation.replace).not.toHaveBeenCalled();
    expect(posts).toHaveLength(1);
    await act(async () => {fireEvent.click(within(notices).getByRole("button", {name: "Resend"}));});
    await waitFor(() => expect(navigation.replace).toHaveBeenCalledWith(`/w/${WORKSPACE}/fleets/${FLEET_ID}`));
    expect(posts.map(post => post.message)).toEqual([MESSAGE, MESSAGE]);
    expect(posts[1]?.operation_id).not.toBe(posts[0]?.operation_id);
    expect(screen.queryByRole("list", {name: "Unsent messages"})).toBeNull();
  });

  it("sends an unconfirmed create again as itself, so the daemon answers with the fleet it may hold", async () => {
    answers.push(() => {throw new TypeError("connection reset");});
    mount();
    await send(MESSAGE);
    expect(await screen.findByText("Sending was interrupted.")).toBeTruthy();
    await send(MESSAGE);
    await waitFor(() => expect(navigation.replace).toHaveBeenCalledOnce());
    expect(posts).toHaveLength(2);
    expect(posts[1]?.operation_id).toBe(posts[0]?.operation_id);
  });

  it("sends changed words after an unconfirmed create as a new operation", async () => {
    answers.push(() => {throw new TypeError("connection reset");});
    mount();
    await send(MESSAGE);
    await screen.findByText("Sending was interrupted.");
    await send(`${MESSAGE} Ignore staging.`);
    await waitFor(() => expect(navigation.replace).toHaveBeenCalledOnce());
    expect(posts[1]?.operation_id).not.toBe(posts[0]?.operation_id);
  });

  it("asks an expired session to sign in, and dismisses the unsent message with its reason", async () => {
    answers.push(() => response({user_message: "Sign in again."}, 401));
    mount();
    await send(MESSAGE);
    expect(await screen.findByRole("link", {name: "Sign in"})).toBeTruthy();
    expect(screen.getAllByRole("alert").map(alert => alert.textContent).join(" ")).toContain("sign-in session has expired");
    await act(async () => {fireEvent.click(screen.getByRole("button", {name: "Dismiss"}));});
    expect(screen.queryByRole("list", {name: "Unsent messages"})).toBeNull();
    expect(screen.queryAllByRole("alert")).toEqual([]);
  });

  it("carries the chosen model, and sends nothing while a create is out", async () => {
    const held = Promise.withResolvers<Response>();
    answers.push(() => held.promise);
    mount();
    expect(screen.getByRole("combobox", {name: "Fleet model"}).textContent).toContain("Workspace model");
    fireEvent.click(screen.getByRole("combobox", {name: "Fleet model"}));
    fireEvent.click(await screen.findByRole("option", {name: MODEL.label}));
    expect(screen.getByRole("combobox", {name: "Fleet model"}).textContent).toContain(MODEL.label);
    await send(MESSAGE);
    expect(await screen.findByText("Starting your fleet…")).toBeTruthy();
    expect(screen.getByRole("button", {name: starters[0]?.title})).toHaveProperty("disabled", true);
    await act(async () => held.resolve(CREATED()));
    await waitFor(() => expect(navigation.replace).toHaveBeenCalledOnce());
    expect(posts).toEqual([{message: MESSAGE, operation_id: expect.stringMatching(UUID_V7), model_ref: MODEL.ref}]);
  });

  it("puts a starter's words in the composer without sending them", async () => {
    mount();
    for (const starter of starters) expect(screen.getByRole("button", {name: starter.title})).toBeTruthy();
    await act(async () => {fireEvent.click(screen.getByRole("button", {name: "Watch a pull request"}));});
    expect(composer().value).toBe(starters[1]?.prompt);
    expect(composer().value).not.toMatch(/completion questions|save the fleet/i);
    expect(posts).toEqual([]);
  });

  it("returns the words when the browser cannot name the message", async () => {
    mint.fail = true;
    mount();
    await send(MESSAGE);
    expect(await screen.findByText(/cannot name the message/)).toBeTruthy();
    expect(composer().value).toBe(MESSAGE);
    expect(posts).toEqual([]);
  });
});

describe("the creation owner", () => {
  const appended = (text: string): AppendMessage => ({createdAt: new Date("2026-11-25T10:00:00Z"), role: "user", parentId: null, sourceId: null,
    content: [{type: "text", text}], attachments: [], runConfig: {}, metadata: {custom: {}}});

  it("sends nothing for an empty message, and ignores a Resend or Dismiss for another operation", async () => {
    answers.push(REFUSED);
    const {result} = renderHook(() => useFleetCreation(WORKSPACE, null));
    await act(async () => {await result.current.onNew(appended(""));});
    expect(posts).toEqual([]);
    await act(async () => {await result.current.onNew(appended(MESSAGE)).catch(() => undefined);});
    const unsent = result.current.pending[0];
    expect(unsent).toMatchObject({text: MESSAGE, state: "refused"});
    act(() => {result.current.resend("another-operation"); result.current.dismiss("another-operation");});
    expect(posts).toHaveLength(1);
    expect(result.current.pending).toEqual([unsent]);
  });

  it("makes one create for two Resends of one refusal, and names a Resend it cannot mint", async () => {
    const held = Promise.withResolvers<Response>();
    answers.push(REFUSED, () => held.promise);
    const {result} = renderHook(() => useFleetCreation(WORKSPACE, null));
    await act(async () => {await result.current.onNew(appended(MESSAGE)).catch(() => undefined);});
    const operation = result.current.pending[0]?.operationId ?? "";
    act(() => {result.current.resend(operation); result.current.resend(operation);});
    await waitFor(() => expect(result.current.creating).toBe(true));
    expect(posts).toHaveLength(2);
    await act(async () => held.resolve(REFUSED()));
    mint.fail = true;
    act(() => result.current.resend(result.current.pending[0]?.operationId ?? ""));
    expect(result.current.error).toMatch(/cannot name the message/);
    expect(posts).toHaveLength(2);
  });

  it("leaves an answer that arrives after the page left unread", async () => {
    const held = Promise.withResolvers<Response>();
    answers.push(() => held.promise);
    const {result, unmount} = renderHook(() => useFleetCreation(WORKSPACE, null));
    const sending = result.current.onNew(appended(MESSAGE));
    await waitFor(() => expect(posts).toHaveLength(1));
    unmount();
    held.resolve(CREATED());
    await expect(sending).rejects.toThrow();
    expect(navigation.replace).not.toHaveBeenCalled();
  });
});
