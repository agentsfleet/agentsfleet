import { afterEach, describe, expect, test } from "bun:test";

import {
  PLATFORM_OPS_FIXTURE_NAME,
} from "./acceptance/fixtures/constants.ts";
import {
  buildPlatformOpsContent,
  onboardUploadTemplate,
} from "./acceptance/fixtures/template-ops.ts";

const ORIGINAL_FETCH = globalThis.fetch;
const ONBOARD_TIMEOUT_MS = 12_345;
const UNIQUE_FLEET_NAME = "fixture-name-replacement-check";
const EXPECTED_NAME_LINE = `name: ${UNIQUE_FLEET_NAME}`;
const ORIGINAL_NAME_LINE = `name: ${PLATFORM_OPS_FIXTURE_NAME}`;
const API_URL = "https://api.test";
const TEMPLATE_ID = "template-1";
const TEST_TOKEN = "test-token";
const WORKSPACE_ID = "workspace-1";
const SKILL_MARKDOWN = "skill";
const TRIGGER_MARKDOWN = "trigger";

afterEach(() => {
  globalThis.fetch = ORIGINAL_FETCH;
});

describe("acceptance template onboarding", () => {
  test("replaces the declared name in both platform operations files", async () => {
    const content = await buildPlatformOpsContent(UNIQUE_FLEET_NAME);

    expect(content.skillMarkdown).toContain(EXPECTED_NAME_LINE);
    expect(content.triggerMarkdown).toContain(EXPECTED_NAME_LINE);
    expect(content.skillMarkdown).not.toContain(ORIGINAL_NAME_LINE);
    expect(content.triggerMarkdown).not.toContain(ORIGINAL_NAME_LINE);
  });

  test("bounds the request with the caller timeout", async () => {
    let observedSignal: AbortSignal | null | undefined;
    globalThis.fetch = (async (_input, init) => {
      observedSignal = init?.signal;
      return new Response(JSON.stringify({ id: TEMPLATE_ID }), { status: 200 });
    }) as typeof globalThis.fetch;

    const id = await onboardUploadTemplate(
      {
        apiUrl: API_URL,
        token: TEST_TOKEN,
        workspaceId: WORKSPACE_ID,
      },
      {
        skillMarkdown: SKILL_MARKDOWN,
        triggerMarkdown: TRIGGER_MARKDOWN,
      },
      ONBOARD_TIMEOUT_MS,
    );

    expect(id).toBe(TEMPLATE_ID);
    expect(observedSignal).toBeInstanceOf(AbortSignal);
    expect(observedSignal?.aborted).toBe(false);
  });
});
