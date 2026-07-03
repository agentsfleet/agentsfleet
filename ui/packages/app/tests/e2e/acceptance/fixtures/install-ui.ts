/**
 * Dashboard template-gallery install: drives the `/fleets/new` template-only
 * flow like a real human. M103 removed paste/github-import authoring — a fleet
 * is now installed from a template card, so this helper first onboards a tenant
 * template over the API (there is no UI/CLI onboard verb), then drives the
 * gallery → confirm → live-states walk in the browser. Used by the
 * full-lifecycle scenarios, which deliberately install through the interface
 * rather than via API seeding so the whole signup → install → observe → halt
 * walk is browser-driven end-to-end.
 *
 * The onboard is the same wire as `agentsfleet install` resolves against:
 * POST /v1/workspaces/{ws}/fleet-templates with `{source_kind:"upload",
 * skill_markdown, trigger_markdown}`. agentsfleetd parses the markdown
 * frontmatter server-side and, by the seed convention, the returned template
 * `id` equals the SKILL.md `name:` — which is also the name the gallery card
 * renders, so the click below targets exactly the card we just onboarded.
 *
 * On `install:ready` the live states surface "Open fleet →", whose click calls
 * `router.push("/fleets/${fleet_id}")`; this helper waits for that navigation
 * and returns the new fleet id.
 */
import * as crypto from "node:crypto";
import { expect, type Page } from "@playwright/test";
import { SOURCE_KIND_UPLOAD } from "@/lib/types";
import { clientFor, type ClientHandle } from "./api-client";

// 60s, not 30s — the install now runs inline through the live SSE state stream
// (creating → provisioning → ready) before "Open fleet →" appears, not just a
// single server action. A tighter timeout false-fails the spec without
// exercising any product behavior; it still sits well inside each scenario's
// FLOW_TIMEOUT budget.
const INSTALL_TIMEOUT_MS = 60_000;

// Auth context the onboard call needs: which fixture identity makes the call
// (a cached FixtureKey or an ephemeral `{sessionJwt}` handle) and the workspace
// whose gallery the install will then resolve the template from. The workspace
// MUST be the one active in the browser at install time, or the onboarded card
// will not render on `/fleets/new`.
export interface InstallAuth {
  handle: ClientHandle;
  workspaceId: string;
}

function fixtureTriggerMd(name: string): string {
  // Use cron here so browser scenarios keep a concrete wake rule.
  return [
    "---",
    `name: ${name}`,
    "x-agentsfleet:",
    "  triggers:",
    "    - type: cron",
    '      schedule: "0 0 * * *"',
    "  tools:",
    "    - agentmail",
    "  budget:",
    "    daily_dollars: 1.0",
    "---",
    "",
  ].join("\n");
}

function fixtureSkillMd(name: string): string {
  return [
    "---",
    `name: ${name}`,
    `description: Fixture skill body for full-lifecycle e2e scenario (${name}).`,
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
  name?: string;
}

// Onboard a fresh tenant template (upload kind) into the install workspace and
// return its id. Declares no `credentials:` block, so the install's connect
// gate is satisfied immediately and the states auto-create the fleet.
async function onboardTemplate(auth: InstallAuth, templateName: string): Promise<string> {
  const client = clientFor(auth.handle);
  const resp = await client.post<OnboardTemplateResp>(
    `/v1/workspaces/${auth.workspaceId}/fleet-templates`,
    {
      source_kind: SOURCE_KIND_UPLOAD,
      skill_markdown: fixtureSkillMd(templateName),
      trigger_markdown: fixtureTriggerMd(templateName),
    },
  );
  if (!resp.id) {
    throw new Error(`installViaUI: template onboard returned no id (${JSON.stringify(resp)})`);
  }
  return resp.id;
}

export async function installViaUI(page: Page, name: string, auth: InstallAuth): Promise<string> {
  // Onboard a uniquely-named tenant template so the gallery has exactly one
  // card we can disambiguate (workspaces accumulate templates across runs —
  // cleanup only deletes fleets). By the seed convention the SKILL `name:` is
  // both the returned id and the card's rendered name.
  const templateName = `tmpl-${crypto.randomBytes(4).toString("hex")}`;
  await onboardTemplate(auth, templateName);

  await page.goto("/fleets/new");
  await expect(page).toHaveURL(/\/fleets\/new(\?|$)/);

  // Gallery → confirm: click this template's card action, then name the fleet
  // and Install. Scope to the card's <article> so the click targets the right
  // "Use template" among any sibling cards.
  const card = page.getByRole("article").filter({ hasText: templateName });
  await card.getByRole("button", { name: "Use template" }).click();
  await page.getByLabel("Fleet name").fill(name);
  await page.getByRole("button", { name: "Install", exact: true }).click();

  // Install runs inline through the live "Install states" stream; on
  // install:ready it surfaces "Open fleet →", whose click does
  // router.push(`/fleets/${fleet_id}`). Wait for the stream to complete (the
  // slow beat), then click through to the detail page.
  await page.getByRole("button", { name: /open fleet/i }).click({ timeout: INSTALL_TIMEOUT_MS });

  // Success path: router.push(`/fleets/${fleet_id}`). Exclude the
  // /fleets/new sentinel so we don't false-match an install that failed and
  // stayed on the form. Use expect.toHaveURL (URL-polling) rather than
  // waitForURL: Next App Router's router.push is a soft Single-Page
  // Application navigation that mutates history without re-firing the
  // document `load` event, so waitForURL's default waitUntil:"load" hangs
  // even after the URL changes.
  await expect(page).toHaveURL(/\/fleets\/(?!new)[a-z0-9-]+(\?|$)/, { timeout: INSTALL_TIMEOUT_MS });
  const id = new URL(page.url()).pathname.split("/").pop();
  if (!id) throw new Error(`installViaUI: could not extract fleet id from ${page.url()}`);
  return id;
}
