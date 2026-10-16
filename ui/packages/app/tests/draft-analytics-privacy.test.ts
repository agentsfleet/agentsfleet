// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { EVENTS } from "@/lib/analytics/events";
import { DRAFT_ID, WORKSPACE } from "./helpers/draft-fixtures";

const sdk = vi.hoisted(() => ({init: vi.fn(), capture: vi.fn()}));
vi.mock("posthog-js", () => ({default: sdk}));
const PATH = `/w/${WORKSPACE}/fleets/new`;
const PRIVATE_TEXT = "private conversation content";
const PRIVATE_URL = "https://private.example.test/repository";
const PRIVATE_SECRET = "synthetic-private-value";

beforeEach(() => {
  vi.resetModules(); vi.clearAllMocks();
  vi.stubEnv("NEXT_PUBLIC_POSTHOG_KEY", "phc_test");
  vi.stubEnv("NEXT_PUBLIC_POSTHOG_ENABLED", "true");
  vi.stubGlobal("window", {location: {pathname: PATH, search: `?message=${PRIVATE_TEXT}`, hash: PRIVATE_SECRET}});
  vi.stubGlobal("requestIdleCallback", (callback: () => void) => {callback(); return 1;});
});
afterEach(() => {vi.unstubAllGlobals(); vi.unstubAllEnvs();});

it("drops private content and unknown fields from a widened review-event payload", async () => {
  const analytics = await import("@/lib/analytics/posthog");
  await analytics.initAnalytics();
  const payload = {workspace_id: WORKSPACE, draft_id: DRAFT_ID, text: PRIVATE_TEXT,
    title: PRIVATE_TEXT, definition: {instructions_md: PRIVATE_TEXT}, url: PRIVATE_URL, secret: PRIVATE_SECRET};
  analytics.captureProductEvent(EVENTS.draft_review_opened, payload);
  expect(sdk.capture).toHaveBeenCalledExactlyOnceWith(EVENTS.draft_review_opened,
    {path: PATH, workspace_id: WORKSPACE, draft_id: DRAFT_ID});
  for (const value of [PRIVATE_TEXT, PRIVATE_URL, PRIVATE_SECRET]) {
    expect(JSON.stringify(sdk.capture.mock.calls)).not.toContain(value);
  }
});

it("records a source preview without inventing a draft identifier", async () => {
  const analytics = await import("@/lib/analytics/posthog");
  await analytics.initAnalytics();
  analytics.captureProductEvent(EVENTS.draft_review_opened, {workspace_id: WORKSPACE, draft_id: undefined});
  expect(sdk.capture).toHaveBeenCalledExactlyOnceWith(EVENTS.draft_review_opened, {path: PATH, workspace_id: WORKSPACE});
});
