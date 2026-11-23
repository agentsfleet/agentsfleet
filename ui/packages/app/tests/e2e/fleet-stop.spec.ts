import { expect, type Route } from "@playwright/test";
import { test } from "./fixtures/chat-probe";
import { STOP_TURNS } from "./fixtures/fleet-stop-data";
import { FLEET_ID, WORKSPACE } from "@/tests/helpers/draft-fixtures";

// M253 §8, Dimension 8.6: the composer shows Stop only while a turn runs. The
// fixture mounts the production thread, composer and Stop; these routes stand
// in for the daemon's turn reads and the turn PATCH.
const TURN_ROUTE = `**/live/v1/workspaces/${WORKSPACE}/fleets/${FLEET_ID}/turns/*`;
const ANSWER = "The staging deploy key is rotated.";

type Saved = {status: string; response_text: string | null};

function row(turnId: string, saved: Saved) {
  return {workspace_id: WORKSPACE, fleet_id: FLEET_ID, turn_id: turnId, actor: "steer:fixture-owner", event_type: "chat",
    request_json: JSON.stringify({message: "fixture"}), tool_calls: null, tokens: null, wall_ms: null, cost_nanos: null,
    failure_label: null, failure_detail: null, resumes_turn_id: null,
    created_at: new Date().toISOString(), updated_at: new Date().toISOString(), ...saved};
}

test("composer_shows_stop_while_a_turn_runs", async ({page, chatProbe}) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  const saved = new Map<string, Saved>([
    [STOP_TURNS.running, {status: "received", response_text: null}],
    [STOP_TURNS.queued, {status: "queued", response_text: null}],
  ]);
  const stops: {turn: string; body: unknown}[] = [];
  await page.route(TURN_ROUTE, async (route: Route) => {
    const turn = new URL(route.request().url()).pathname.split("/").at(-1) ?? "";
    const current = saved.get(turn);
    if (current === undefined) return route.fulfill({status: 404, json: {error_code: "UZ-AGT-015"}});
    if (route.request().method() === "PATCH") {
      stops.push({turn, body: route.request().postDataJSON()});
      saved.set(turn, {status: "cancelled", response_text: null});
    }
    return route.fulfill({json: row(turn, saved.get(turn) ?? current)});
  });
  await page.goto(`${chatProbe}?fleetStop=1`);
  const stop = page.getByRole("button", {name: "Stop", exact: true});
  const composer = page.getByRole("textbox");
  // The transcript itself; the settled-reply announcement repeats each answer.
  const log = page.getByRole("log", {name: "Chat"});
  await expect(composer).toBeVisible();

  // The first turn runs: Stop shows beside Send.
  await expect(stop).toBeVisible();
  await expect(page.getByRole("button", {name: "Send", exact: true})).toBeVisible();

  // Its answer lands. The queued turn behind it is next, so Stop stays for it.
  saved.set(STOP_TURNS.running, {status: "processed", response_text: ANSWER});
  await expect(log.getByText(ANSWER)).toBeVisible();
  await expect(stop).toBeVisible();

  // Stop sends the PATCH for the queued turn, which reads Stopped; no turn runs, so Stop goes.
  await stop.click();
  await expect(log.getByText("Stopped.")).toBeVisible();
  await expect(stop).toHaveCount(0);
  expect(stops).toEqual([{turn: STOP_TURNS.queued, body: {status: "cancelled"}}]);
  expect(errors).toEqual([]);
});
