import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ZodError } from "zod";
import { ApiError } from "@/lib/api/errors";

const {read, refresh} = vi.hoisted(() => ({read: vi.fn(), refresh: vi.fn()}));
vi.mock("@/lib/auth/credential", () => ({credential: async () => "session", requireCredential: async () => "session"}));
vi.mock("@/lib/api/events", () => ({listWorkspaceEvents: read}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh})}));
vi.mock("@/components/domain/EventsList", () => ({EventsList: () => <p>Recorded activity</p>}));
import { EventsData } from "@/app/(dashboard)/w/[workspaceId]/events/page";

afterEach(cleanup);
beforeEach(() => vi.clearAllMocks());

it.each([
  [401, "Sign in", "/sign-in"],
  [403, "Choose workspace", "/"],
  [404, "Choose workspace", "/"],
  [400, "Latest activity", "/w/workspace/events"],
])("gives a useful recovery action for refused activity (%i)", async (status, label, href) => {
  read.mockRejectedValue(new ApiError("private server detail", status, "READ_FAILED"));
  render(await EventsData({workspaceId: "workspace", cursor: "old-page"}));
  expect(screen.getByRole("link", {name: label}).getAttribute("href")).toBe(href);
  expect(screen.queryByRole("button", {name: "Retry"})).toBeNull();
  expect(screen.queryByText("Recorded activity")).toBeNull();
  expect(document.body.textContent).not.toContain("private server detail");
});

it.each([new ApiError("unavailable", 503, "UNAVAILABLE"), new ApiError("invalid", 400, "INVALID"), new ZodError([])])(
  "keeps a failed read distinct from empty activity and allows retry", async (error) => {
    read.mockRejectedValue(error);
    render(await EventsData({workspaceId: "workspace"}));
    expect(screen.getByRole("alert").textContent).toContain("Activity couldn’t be loaded");
    fireEvent.click(screen.getByRole("button", {name: "Retry"}));
    expect(refresh).toHaveBeenCalledOnce();
    expect(screen.queryByText("Recorded activity")).toBeNull();
  },
);

it("keeps unexpected programming failures visible to the route boundary", async () => {
  read.mockRejectedValue(new Error("programming failure"));
  await expect(EventsData({workspaceId: "workspace"})).rejects.toThrow("programming failure");
});
