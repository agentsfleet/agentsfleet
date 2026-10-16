import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { WorkspaceChat } from "@/components/domain/chat/WorkspaceChat";
import { response, WORKSPACE } from "@/tests/helpers/draft-fixtures";

let fixture: ChatFixture;
beforeEach(() => { fixture = new ChatFixture(); fixture.mount(); });
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

it("explains unavailable setup, preserves the lock, and offers reload or fleet navigation", async () => {
  fixture.fetch.mockResolvedValueOnce(response({}, 404));
  const reload = vi.spyOn(window.location, "reload").mockImplementation(() => undefined);
  render(<WorkspaceChat {...CHAT_PROPS} />);
  expect((await screen.findByRole("alert")).textContent).toContain("Fleet setup isn’t available");
  expect(screen.getByRole("link", {name: "Your fleets"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets`);
  const suggestion = await screen.findByRole("button", {name: /Fix a build/});
  expect(suggestion).toHaveProperty("disabled", true);
  fireEvent.click(screen.getByRole("button", {name: "Reload"}));
  expect(reload).toHaveBeenCalledOnce();
  expect(fixture.writes()).toHaveLength(0);
});

it("retries a failed initial read without claiming the workspace has no models", async () => {
  fixture.fetch.mockResolvedValueOnce(response({}, 503));
  render(<WorkspaceChat {...CHAT_PROPS} />);
  await screen.findByText("We couldn’t load your models and chats. Try again in a moment.");
  expect(screen.queryByText(/Add a model to this workspace/)).toBeNull();
  fireEvent.click(screen.getByRole("button", {name: "Retry"}));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  await waitFor(() => expect(screen.getByRole("button", {name: /Fix a build/})).toHaveProperty("disabled", false));
  expect(fixture.writes()).toHaveLength(0);
});


it("shows an oversized message refusal inside the chat without creating a draft", async () => {
  vi.spyOn(console, "error").mockImplementation(() => undefined);
  render(<WorkspaceChat {...CHAT_PROPS} />);
  const input = await screen.findByRole("textbox", {name: "Describe the work you want this fleet to do…"});
  await waitFor(() => expect(screen.getByRole("button", {name: /Fix a build/})).toHaveProperty("disabled", false));
  fireEvent.change(input, {target: {value: "🌱".repeat(8_193)}});
  fireEvent.click(screen.getByRole("button", {name: "Send message"}));
  expect((await screen.findByRole("alert")).textContent).toContain("Messages must contain 1 to 32,768 bytes of text.");
  expect(fixture.writes()).toEqual([]);
});


it("offers model settings when a successful list confirms there are no authoring models", async () => {
  const base = fixture.fetch.getMockImplementation();
  if (!base) throw new Error("Chat fetch fixture is not installed");
  fixture.fetch.mockImplementation((input, init) => typeof input === "string" && input.includes("/models?")
    ? Promise.resolve(response({items: [], next_cursor: null})) : base(input, init));
  render(<WorkspaceChat {...CHAT_PROPS} />);
  await screen.findByText("Add a model to this workspace to start chatting.");
  expect(screen.getByRole("link", {name: "Open model settings"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/settings/models`);
  expect(screen.getByRole("button", {name: "Send message"})).toHaveProperty("disabled", true);
});
