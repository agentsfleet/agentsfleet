import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ChatFixture, CHAT_PROPS } from "./fixture";
import { WorkspaceChat } from "@/components/domain/chat/WorkspaceChat";
import { EVENTS } from "@/lib/analytics/events";
import { DRAFT_ID, WORKSPACE } from "@/tests/helpers/draft-fixtures";

const capture = vi.hoisted(() => vi.fn());
vi.mock("@/lib/analytics/posthog", () => ({captureProductEvent: capture}));
const REVIEW_BUTTON = "Review setup";
const REVIEW_DIALOG = "Review fleet setup";
let fixture: ChatFixture;

beforeEach(() => {capture.mockReset(); fixture = new ChatFixture(); fixture.mount();});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

it("records only authorized identifiers when review opens, with no mount or message-content event", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT_ID} />);
  const button = await screen.findByRole("button", {name: REVIEW_BUTTON});
  await waitFor(() => expect(button).toHaveProperty("disabled", false));
  expect(capture).not.toHaveBeenCalled();
  fireEvent.click(button);
  await screen.findByRole("dialog", {name: REVIEW_DIALOG});
  expect(capture).toHaveBeenCalledExactlyOnceWith(EVENTS.draft_review_opened, {workspace_id: WORKSPACE, draft_id: DRAFT_ID});
  expect(JSON.stringify(capture.mock.calls)).not.toContain(fixture.draft.title);
  expect(fixture.saveAttempts).toBe(0);
});

it("records review intent even when the subsequent refresh fails", async () => {
  render(<WorkspaceChat {...CHAT_PROPS} draft={DRAFT_ID} />);
  const button = await screen.findByRole("button", {name: REVIEW_BUTTON});
  await waitFor(() => expect(button).toHaveProperty("disabled", false));
  fixture.fetch.mockRejectedValue(new TypeError("network unavailable"));
  fireEvent.click(button);
  await screen.findByRole("dialog", {name: REVIEW_DIALOG});
  await screen.findAllByText(/request could not be confirmed/);
  expect(capture).toHaveBeenCalledExactlyOnceWith(EVENTS.draft_review_opened, {workspace_id: WORKSPACE, draft_id: DRAFT_ID});
  expect(fixture.saveAttempts).toBe(0);
});
