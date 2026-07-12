import { expect, test } from "bun:test";

import {
  PLATFORM_OPS_FIXTURE_NAME,
} from "./acceptance/fixtures/constants.ts";
import { buildPlatformOpsContent } from "./acceptance/fixtures/template-ops.ts";

const UNIQUE_FLEET_NAME = "fixture-name-replacement-check";
const EXPECTED_NAME_LINE = `name: ${UNIQUE_FLEET_NAME}`;
const ORIGINAL_NAME_LINE = `name: ${PLATFORM_OPS_FIXTURE_NAME}`;

test("platform operations content replaces the declared name in both files", async () => {
  const content = await buildPlatformOpsContent(UNIQUE_FLEET_NAME);

  expect(content.skillMarkdown).toContain(EXPECTED_NAME_LINE);
  expect(content.triggerMarkdown).toContain(EXPECTED_NAME_LINE);
  expect(content.skillMarkdown).not.toContain(ORIGINAL_NAME_LINE);
  expect(content.triggerMarkdown).not.toContain(ORIGINAL_NAME_LINE);
});
