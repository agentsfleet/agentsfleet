import type {Page, TestInfo} from "@playwright/test";
import {writeFile} from "node:fs/promises";
import {visualProvenance} from "../../fixtures/visual-provenance";
import {measureVisualState} from "../../fixtures/visual-measurement";

/** Real authenticated forms, including unsaved edits; no credential values. */
export async function captureAccountState(page: Page, info: TestInfo, state: string, width: number, theme: string) {
  const source = await visualProvenance();
  await page.evaluate(() => document.fonts.ready);
  const geometry = await measureVisualState(page);
  const name = `authenticated-account-${state}-${width}-${theme}`;
  await page.screenshot({path: info.outputPath(`${name}.png`), animations: "disabled"});
  await writeFile(info.outputPath(`${name}-evidence.json`), JSON.stringify({...source,
    captureTimeUtc: new Date().toISOString(), evidenceKind: "authenticated-account", state, width, theme, geometry}, null, 2));
}
