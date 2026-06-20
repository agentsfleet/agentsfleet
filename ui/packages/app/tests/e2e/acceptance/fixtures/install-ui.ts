/**
 * Dashboard-form install: drives the `/fleets/new` `InstallFleetForm` like a
 * real human. Used by the full-lifecycle scenarios, which deliberately
 * drive the install through the interface rather than via API seeding so the entire
 * signup → install → observe → halt walk is browser-driven end-to-end.
 *
 * Same wire as `agentsfleet install --from`: the form takes SKILL.md and
 * TRIGGER.md bodies; agentsfleetd parses the markdown frontmatter server-side and
 * derives `name` plus the compiled trigger config from it. The helper just
 * pastes valid markdown and clicks.
 *
 * On success the form calls `router.push("/fleets/${fleet_id}")`; this
 * helper waits for that navigation and returns the new fleet id.
 */
import { expect, type Page } from "@playwright/test";

// 30s, not 15s — the dashboard's `installFleetAction` is a Next.js Server
// Action that compiles on first hit under `next dev --turbopack`. Cold-start
// observed at ~18s in local runs; Continuous Integration (CI) is in the same ballpark. A tighter
// timeout false-fails the spec without exercising any product behavior.
const INSTALL_TIMEOUT_MS = 30_000;

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

export async function installViaUI(page: Page, name: string): Promise<string> {
  await page.goto("/fleets/new");
  await expect(page).toHaveURL(/\/fleets\/new(\?|$)/);

  await page.getByLabel("SKILL.md body").fill(fixtureSkillMd(name));
  await page.getByLabel("TRIGGER.md body").fill(fixtureTriggerMd(name));
  await page.getByRole("button", { name: "Install teammate" }).click();

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
