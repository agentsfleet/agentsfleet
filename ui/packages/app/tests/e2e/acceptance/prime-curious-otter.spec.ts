/** A real search and page read, traced from the public fleet to its dashboard and vendor reservations. */
import {execFileSync} from "node:child_process";
import {fileURLToPath} from "node:url";
import {expect, test, type Page} from "@playwright/test";
import {z} from "zod";
import {signInAs} from "./fixtures/auth";
import {assertPassed, classifyTerminalTurn} from "./fixtures/execution";
import {workspaceHref} from "./fixtures/nav";
import {createPrimeFleet, entry, PRIME_FLEET, primeFleet, proofs} from "./fixtures/prime-fleets";
import {PRIME_PERSON, say} from "./fixtures/prime-walk";
import {getDefaultWorkspaceId} from "./fixtures/seed";
import {purgeFixtureFleet} from "./fixtures/teardown";

const fixture = primeFleet(PRIME_FLEET.curiousOtter);
const proof = proofs(fixture, 1);
const ROOT = fileURLToPath(new URL("../../../../../../", import.meta.url));
const JOURNEY_TIMEOUT_MS = 420_000;
const RENDER_TIMEOUT_MS = 15_000;
const ToolSources = z.array(z.object({url: z.url({protocol: /^https?$/}), title: z.string().nullish()})).min(1);
const Reservation = z.object({run_id: z.uuidv7(), vendor: z.string(), operation: z.string(), call_count: z.number().int()});

// Use the worktree's existing acceptance Compose service and its own database.
// psql binds each identifier as a quoted value. Nothing here needs a vendor key.
function reservations(workspace: string, fleet: string, turn: string) {
  const command = `
    set -eu
    source scripts/local-dev.sh
    dev_state
    dev_compose exec -T postgres psql --no-psqlrc -qAt -U agentsfleet -d "$DEV_DB" -v ON_ERROR_STOP=1 -v workspace="$1" -v fleet="$2" -v turn="$3"
  `;
  const input = `
    SELECT row_to_json(reservation) FROM (
      SELECT r.id AS run_id, w.vendor, w.operation, w.call_count
        FROM thread.runs r
        JOIN thread.run_web_calls w ON w.run_id = r.id
       WHERE r.workspace_id = :'workspace'::uuid AND r.fleet_id = :'fleet'::uuid AND r.turn_id = :'turn'::uuid
       ORDER BY r.id, w.operation, w.vendor
    ) reservation;
  `;
  const rows = execFileSync("bash", ["-c", command, "web-reservations", workspace, fleet, turn], {
    cwd: ROOT, input, encoding: "utf8", stdio: ["pipe", "pipe", "inherit"], timeout: RENDER_TIMEOUT_MS,
  }).trim();
  return rows ? rows.split("\n").map(row => Reservation.parse(JSON.parse(row))) : [];
}

/** Scope citations to the assistant's chat message, independently of the Sources panel. */
async function expectAnswerInChat(page: Page, question: string, firstPage: string) {
  const chat = page.getByRole("log", {name: "Chat"});
  await expect(chat.locator('[data-role="user"]').filter({hasText: question})).toBeVisible({timeout: RENDER_TIMEOUT_MS});
  const answer = chat.locator('[data-role="assistant"]').last();
  const explanation = answer.locator("p").filter({hasText: /reserv|document|illustrat|designat|set aside/i}).first();
  await expect(explanation).toBeVisible({timeout: RENDER_TIMEOUT_MS});
  const citation = answer.locator(`a[href=${JSON.stringify(firstPage)}]`).first();
  await expect(citation).toBeVisible({timeout: RENDER_TIMEOUT_MS});
  await expect(citation).toHaveAttribute("href", firstPage);
}

async function expectSources(page: Page, expected: ReadonlyMap<string, string>) {
  const sources = page.getByRole("region", {name: "Sources", exact: true});
  await expect(sources).toBeVisible({timeout: RENDER_TIMEOUT_MS});
  await expect(sources.getByRole("link")).toHaveCount(expected.size, {timeout: RENDER_TIMEOUT_MS});
  for (const [url, label] of expected) {
    const link = sources.locator(`a[href=${JSON.stringify(url)}]`);
    await expect(link).toBeVisible({timeout: RENDER_TIMEOUT_MS});
    await expect(link).toHaveText(label);
  }
}

test.describe(`prime fleet ${fixture.name}`, () => {
  test.setTimeout(JOURNEY_TIMEOUT_MS);

  test(proof(0), async ({page}) => {
    expect(process.env.LOCAL_ACCEPTANCE_STATE, "Requires the local full-stack deployment from make acceptance").toBeTruthy();
    const workspace = await getDefaultWorkspaceId(PRIME_PERSON);
    const fleet = await createPrimeFleet(PRIME_PERSON, workspace, fixture);
    try {
      await signInAs(page, PRIME_PERSON);
      await page.goto(workspaceHref(workspace, `fleets/${fleet.id}`));
      const allowAll = page.getByRole("switch", {name: "Allow all public internet access"});
      await expect(allowAll).not.toBeChecked();
      await allowAll.click();
      await expect(allowAll).toBeChecked({timeout: RENDER_TIMEOUT_MS});

      const question = entry(fixture, fixture.follow_ups, 0, "question");
      const turn = await say(page, workspace, fleet.id, question);
      assertPassed(classifyTerminalTurn(turn));
      const web = (turn.tool_calls?.calls ?? []).filter(call => call.name === "web_search" || call.name === "web_fetch");
      expect(web.map(call => [call.name, call.status])).toEqual([["web_search", "succeeded"], ["web_fetch", "succeeded"]]);
      const [search, fetch] = web;
      if (!search || !fetch) throw new Error("No completed search and page read on the answering turn");
      const searchSources = ToolSources.parse(search.sources);
      const fetchSources = ToolSources.parse(fetch.sources);
      const top = searchSources[0]?.url;
      if (!top) throw new Error("Search returned no first source for the page read");
      expect(fetch.arguments.url).toBe(top);
      expect(fetchSources[0]?.url).toBe(top);
      await expectAnswerInChat(page, question, top);
      const expectedSources = new Map<string, string>();
      for (const {url, title} of [...searchSources, ...fetchSources]) {
        expectedSources.set(url, title?.trim() || expectedSources.get(url) || url);
      }
      await expectSources(page, expectedSources);

      const rows = reservations(workspace, fleet.id, turn.turn_id);
      expect(new Set(rows.map(row => row.run_id)).size).toBe(1);
      expect(rows.map(({vendor, operation, call_count}) => [vendor, operation, call_count])).toEqual([
        ["parallel", "extract", 1], ["parallel", "search", 1],
      ]);

      await page.reload();
      await expectAnswerInChat(page, question, top);
      await expectSources(page, expectedSources);
    } finally {
      await purgeFixtureFleet(PRIME_PERSON, workspace, fleet.id);
    }
  });
});
