/**
 * reduced-motion.spec.ts — the dashboard shell honours prefers-reduced-motion.
 *
 * `app/globals.css` ends every animation and transition at once for an
 * operator who asks the OS for less motion. Only a browser resolves that media
 * query, so this measures the computed durations of every element in the
 * signed-in shell, pseudo-elements included, with the preference emulated.
 * The same walk without the preference must find real motion, or the reduced
 * run would pass on a page that never moved.
 */
import { expect, type Page, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId } from "./fixtures/seed";

const RENDER_TIMEOUT_MS = 15_000;
// The stylesheet ends motion in 0.01ms; the ceiling leaves room for float
// parsing of Chrome's "1e-05s" and still sits far under any visible motion.
const REDUCED_CEILING_MS = 1;
const MS_PER_SECOND = 1000;
const SIDEBAR = "Primary";

type Motion = { element: string; property: string; ms: number };

// Every animation and transition duration in the document longer than the
// reduced-motion ceiling, named by tag and class for the failure message.
function longMotions(page: Page): Promise<Motion[]> {
  return page.evaluate(({ ceiling, msPerSecond }) => {
    const toMs = (value: string) =>
      value.split(",").map((part) => {
        const text = part.trim();
        return text.endsWith("ms") ? Number.parseFloat(text) : Number.parseFloat(text) * msPerSecond;
      });
    const found: Motion[] = [];
    for (const element of Array.from(document.querySelectorAll("*"))) {
      for (const pseudo of [null, "::before", "::after"]) {
        const style = getComputedStyle(element, pseudo);
        const durations = {
          animation: style.animationName === "none" ? [] : toMs(style.animationDuration),
          transition: toMs(style.transitionDuration),
        };
        for (const [property, values] of Object.entries(durations)) {
          for (const ms of values) {
            if (ms > ceiling) found.push({ element: `${element.tagName}${pseudo ?? ""}.${element.getAttribute("class") ?? ""}`, property, ms });
          }
        }
      }
    }
    return found;
  }, { ceiling: REDUCED_CEILING_MS, msPerSecond: MS_PER_SECOND });
}

async function openShell(page: Page): Promise<void> {
  const workspaceId = await getDefaultWorkspaceId("regular");
  await signInAs(page, "regular");
  await page.goto(`/w/${workspaceId}/fleets`);
  await expect(page.getByRole("navigation", { name: SIDEBAR })).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
}

test.describe("the dashboard shell under prefers-reduced-motion", () => {
  test("the shell moves when the operator has not asked for less motion", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await openShell(page);
    expect((await longMotions(page)).length).toBeGreaterThan(0);
  });

  test("every animation and transition ends at once when the operator asks for less motion", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await openShell(page);
    expect(await longMotions(page)).toEqual([]);
  });
});
