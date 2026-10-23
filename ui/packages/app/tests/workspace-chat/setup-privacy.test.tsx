import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS, openSetupReview } from "./fixture";
import { WorkspaceChat } from "@/components/domain/chat/WorkspaceChat";
import { DRAFT_ID } from "@/tests/helpers/draft-fixtures";

const capture = vi.hoisted(() => vi.fn());
vi.mock("@/lib/analytics/posthog", () => ({captureProductEvent: capture}));
let fixture: ChatFixture;
beforeEach(() => {capture.mockReset(); fixture = new ChatFixture(); fixture.mount();});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it("shows setup and publishing controls without sending private text or a review-intent event", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT_ID} />);
  await screen.findByRole("region", {name: "Fleet setup"});
  await openSetupReview();
  fireEvent.click(screen.getByRole("button", {name: "Publish to Fleet library"}));
  await screen.findByLabelText("Library name");
  expect(capture).not.toHaveBeenCalled();
  expect(fixture.writes()).toHaveLength(0);
  expect(screen.getByRole("dialog", {name: "Edit instructions"}).className).toContain("ph-no-capture");
});
