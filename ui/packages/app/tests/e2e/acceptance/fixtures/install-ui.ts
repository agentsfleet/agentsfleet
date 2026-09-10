/**
 * Dashboard template-gallery install: drives the `/w/<id>/fleets/new` template-only
 * flow like a real human. M103 removed paste/github-import authoring — a fleet
 * is now installed from a template card, so this helper first onboards a tenant
 * template over the API (there is no UI/CLI onboard verb), then drives the
 * gallery → live-states walk in the browser. Used by the
 * full-lifecycle scenarios, which deliberately install through the interface
 * rather than via API seeding so the whole signup → install → observe → halt
 * walk is browser-driven end-to-end.
 *
 * The onboard is the same wire as `agentsfleet install` resolves against:
 * POST /v1/workspaces/{ws}/fleet-libraries with `{source_kind:"upload",
 * skill_markdown, trigger_markdown}`. agentsfleetd parses the markdown
 * frontmatter server-side and, by the seed convention, the returned template
 * `id` equals the SKILL.md `name:` — which is also the name the gallery card
 * renders, so the click below targets exactly the card we just onboarded.
 *
 * On `install:ready` the live states surface "Open fleet →", whose click calls
 * `router.push("/w/${workspaceId}/fleets/${fleet_id}")`; this helper waits for
 * that navigation and returns the new fleet id.
 */
import { expect, type Page } from "@playwright/test";
import { SOURCE_KIND_UPLOAD } from "@/lib/types";
import { clientFor, type ClientHandle } from "./api-client";
import { workspaceHref, workspaceUrlPattern } from "./nav";

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
// will not render on `/w/<workspaceId>/fleets/new`.
export interface InstallAuth {
  handle: ClientHandle;
  workspaceId: string;
  // The SKILL.md the onboarded template carries. Absent, the placeholder body
  // below: enough for a lifecycle walk that never delivers. A journey that
  // needs the fleet to actually answer passes a body with instructions in it.
  skillMarkdown?: string;
  // The TRIGGER.md the onboarded template carries. Absent, the cron-only
  // fixture below, which declares no credentials and so clears the connect
  // gate immediately. A journey about credentials passes frontmatter with a
  // `credentials:` block — the workspace must already hold every name it
  // declares, or the install holds at the gate instead of creating.
  triggerMarkdown?: string;
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
    `/v1/workspaces/${auth.workspaceId}/fleet-libraries`,
    {
      source_kind: SOURCE_KIND_UPLOAD,
      skill_markdown: auth.skillMarkdown ?? fixtureSkillMd(templateName),
      trigger_markdown: auth.triggerMarkdown ?? fixtureTriggerMd(templateName),
    },
  );
  if (!resp.id) {
    throw new Error(`installViaUI: template onboard returned no id (${JSON.stringify(resp)})`);
  }
  return resp.id;
}

export async function installViaUI(page: Page, name: string, auth: InstallAuth): Promise<string> {
  // Onboard a tenant template under the caller's STABLE name. The one-step
  // install takes the template's own name (no confirm step, no name field), so
  // the template names the fleet, and the server suffixes a taken name
  // (`{template}-NNN`) rather than refusing it. The name must be the same on
  // every run: an onboard of identical bytes converges on one library row per
  // workspace, where a per-run unique name minted a row per run that nothing
  // deletes — the fixture workspace's gallery reached a hundred of them and
  // pushed the platform catalogue off its first page. Cleanup sweeps fleets by
  // this same prefix, which the suffixed name still carries.
  await onboardTemplate(auth, name);

  await page.goto(workspaceHref(auth.workspaceId, "fleets/new"));
  await expect(page).toHaveURL(workspaceUrlPattern("fleets/new"));

  // One step: click this template's card action — the install starts. Scope to
  // the card's <article> so the click targets the right "Install" among any
  // sibling cards.
  const card = page.getByRole("article").filter({ has: page.getByText(name, { exact: true }) });
  await card.getByRole("button", { name: "Install" }).click();

  // Install runs inline through the live "Install states" stream; on
  // install:ready it surfaces "Open fleet →", whose click does
  // router.push(`/fleets/${fleet_id}`). Wait for the stream to complete (the
  // slow beat), then click through to the detail page.
  await page.getByRole("button", { name: /open fleet/i }).click({ timeout: INSTALL_TIMEOUT_MS });

  // Success path: router.push(`/w/${workspaceId}/fleets/${fleet_id}`). Exclude
  // the /fleets/new sentinel so we don't false-match an install that failed and
  // stayed on the form. Use expect.toHaveURL (URL-polling) rather than
  // waitForURL: Next App Router's router.push is a soft Single-Page
  // Application navigation that mutates history without re-firing the
  // document `load` event, so waitForURL's default waitUntil:"load" hangs
  // even after the URL changes.
  await expect(page).toHaveURL(/\/w\/[^/]+\/fleets\/(?!new)[a-z0-9-]+(\?|$)/, {
    timeout: INSTALL_TIMEOUT_MS,
  });
  const id = new URL(page.url()).pathname.split("/").pop();
  if (!id) throw new Error(`installViaUI: could not extract fleet id from ${page.url()}`);
  return id;
}
