/**
 * Dashboard-form install: drives the `/zombies/new` `InstallZombieForm` like a
 * real operator. Used by the full-lifecycle scenarios, which deliberately
 * drive the install through the UI rather than via API seeding so the entire
 * signup → install → observe → halt walk is browser-driven end-to-end.
 *
 * The form accepts:
 *   - Name              (kebab-case)
 *   - SKILL.md body     (markdown with valid frontmatter)
 *   - Config JSON       (the compiled trigger; `zombiectl install --from`
 *                        does this compile from TRIGGER.md, but the dashboard
 *                        form is the power-user paste path with no compile)
 *
 * Minimal valid payloads — zombied accepts `name: <kebab>` frontmatter +
 * `{name: <same>}` config (mirrors the shape ui/packages/app/tests/zombies.test.ts
 * uses to exercise installZombieAction).
 *
 * On success the form calls `router.push("/zombies/${zombie_id}")`; this
 * helper waits for that navigation and returns the new zombie id.
 */
import { expect, type Page } from "@playwright/test";

const INSTALL_TIMEOUT_MS = 15_000;

function minimalSkillMd(name: string): string {
  return `---\nname: ${name}\n---\n# ${name}\n\nFixture skill body for full-lifecycle e2e scenario.\n`;
}

function minimalConfigJson(name: string): string {
  return JSON.stringify({ name });
}

export async function installViaUI(page: Page, name: string): Promise<string> {
  await page.goto("/zombies/new");
  await expect(page).toHaveURL(/\/zombies\/new(\?|$)/);

  await page.getByLabel("Name", { exact: true }).fill(name);
  await page.getByLabel("SKILL.md body").fill(minimalSkillMd(name));
  await page.getByLabel("Config JSON").fill(minimalConfigJson(name));
  await page.getByRole("button", { name: "Install Zombie" }).click();

  // Success path: router.push(`/zombies/${zombie_id}`). Use a URL regex that
  // excludes the new-zombie sentinel so we don't false-match an install that
  // failed and stayed on /zombies/new.
  await page.waitForURL(/\/zombies\/(?!new)[a-z0-9-]+(\?|$)/, { timeout: INSTALL_TIMEOUT_MS });
  const id = new URL(page.url()).pathname.split("/").pop();
  if (!id) throw new Error(`installViaUI: could not extract zombie id from ${page.url()}`);
  return id;
}
