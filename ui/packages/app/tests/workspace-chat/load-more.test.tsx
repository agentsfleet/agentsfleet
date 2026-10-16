import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { DraftLoadMore, LOAD_MORE_COPY } from "@/components/domain/chat/DraftLoadMore";
import { RouteRetryButton } from "@/components/domain/RouteRetryButton";

const { refresh } = vi.hoisted(() => ({refresh: vi.fn()}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh})}));
afterEach(() => {cleanup(); vi.clearAllMocks();});

it("locks an append request, explains failure, and clears that failure after retry", async () => {
  const first = Promise.withResolvers<void>();
  const load = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValueOnce(undefined);
  render(<DraftLoadMore copy={LOAD_MORE_COPY.chats} onLoad={load} />);
  fireEvent.click(screen.getByRole("button", {name: "Load more chats"}));
  expect(screen.getByRole("button", {name: "Loading…"})).toHaveProperty("disabled", true);
  await act(async () => first.reject(new Error("Network refused")));
  expect(screen.getByRole("alert").textContent).toContain(LOAD_MORE_COPY.chats.failure);
  fireEvent.click(screen.getByRole("button", {name: "Load more chats"}));
  await screen.findByRole("button", {name: "Load more chats"});
  expect(screen.queryByRole("alert")).toBeNull();
});

it("keeps a route retry visibly pending until the router refresh settles", async () => {
  const pending = Promise.withResolvers<void>();
  refresh.mockReturnValue(pending.promise);
  render(<RouteRetryButton label="Retry library" />);
  fireEvent.click(screen.getByRole("button", {name: "Retry library"}));
  expect(screen.getByRole("button", {name: "Retrying…"})).toHaveProperty("disabled", true);
  await act(async () => pending.resolve());
  expect(screen.getByRole("button", {name: "Retry library"})).toHaveProperty("disabled", false);
  expect(refresh).toHaveBeenCalledOnce();
});
