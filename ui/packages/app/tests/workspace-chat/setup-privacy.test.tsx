import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { WorkspaceChat } from "@/components/domain/setup/conversation/WorkspaceChat";
import { DRAFT_ID } from "@/tests/helpers/draft-fixtures";

const capture = vi.hoisted(() => vi.fn());
vi.mock("@/lib/analytics/posthog", () => ({captureProductEvent: capture}));
let fixture: ChatFixture;
beforeEach(() => {capture.mockReset(); fixture = new ChatFixture(); fixture.mount();});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it("shows setup and save controls without sending private text or a review-intent event", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT_ID} />);
  await screen.findByRole("region", {name: "Fleet setup"});
  await screen.findByRole("button", {name: "Save"});
  expect(capture).not.toHaveBeenCalled();
  expect(fixture.writes()).toHaveLength(0);
  expect(screen.queryByRole("dialog")).toBeNull();
});
