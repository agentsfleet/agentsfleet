/**
 * template-onboarding.spec.ts — template onboarding → tenant gallery render.
 *
 * Drives the dashboard path M110 adds: an authenticated tenant owner opens the
 * template gallery and sees an onboarded tenant template beside platform
 * templates. The public GitHub dialog error path is covered against the live
 * API; the success path uses an upload fixture because the public agentsfleet
 * template repos do not currently expose root-level SKILL.md files.
 */
import * as crypto from "node:crypto";
import { expect, test } from "@playwright/test";
import { SOURCE_KIND_UPLOAD } from "@/lib/types";
import { clientFor } from "./fixtures/api-client";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";

const INVALID_GITHUB_SOURCE_REF = "agentsfleet/github-pr-reviewer";
const FLOW_TIMEOUT_MS = 120_000;

function fixtureSkillMd(name: string): string {
  return [
    "---",
    `name: ${name}`,
    `description: End-to-End fixture template ${name}.`,
    "version: 0.1.0",
    "---",
    "",
    `# ${name}`,
    "",
    "Fixture body.",
    "",
  ].join("\n");
}

interface OnboardTemplateResp {
  id: string;
}

test.describe("template onboarding", () => {
  test.setTimeout(FLOW_TIMEOUT_MS);

  test("test_onboarded_template_renders_in_gallery", async ({ page }) => {
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const templateName = `tmpl-${crypto.randomBytes(4).toString("hex")}`;
    const client = clientFor(FIXTURE_KEY.regular);
    const resp = await client.post<OnboardTemplateResp>(
      `/v1/workspaces/${workspaceId}/fleet-templates`,
      {
        source_kind: SOURCE_KIND_UPLOAD,
        skill_markdown: fixtureSkillMd(templateName),
      },
    );
    expect(resp.id.length).toBeGreaterThan(0);

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/fleets/new");
    await expect(page).toHaveURL(/\/fleets\/new(\?|$)/);

    const card = page.getByRole("article").filter({ hasText: templateName });
    await expect(card).toBeVisible({ timeout: FLOW_TIMEOUT_MS });
    await expect(card.getByRole("button", { name: "Use template" })).toBeVisible();
  });

  test("test_github_source_error_stays_in_dialog", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/fleets/new");
    await expect(page).toHaveURL(/\/fleets\/new(\?|$)/);

    await page.getByRole("button", { name: "Create a template" }).first().click();
    const dialog = page.getByRole("dialog", { name: "Create a template" });
    await expect(dialog).toBeVisible();
    await dialog.getByLabel("Repository").fill(INVALID_GITHUB_SOURCE_REF);
    await dialog.getByRole("button", { name: /^create template$/i }).click();

    await expect(dialog).toBeVisible({ timeout: FLOW_TIMEOUT_MS });
    await expect(dialog.getByRole("alert")).toContainText("Couldn't add the template");
  });
});
