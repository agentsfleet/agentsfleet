/**
 * wall-tile.ts — reading one wall tile's footer figures out of a live page.
 *
 * The wall's tiles are advanced by stream frames, not by a fetch, so the only
 * honest way to ask "did this tile's numbers move?" is to read the rendered
 * figures twice on a page nothing navigated. That is what this module is for,
 * and why it parses the tile's own text rather than reaching into the store: a
 * walk that read the store would pass while the tile rendered a stale snapshot,
 * which is precisely the defect the cross-session journey exists to catch.
 *
 * The tile copy is MIRRORED here rather than imported. `FleetTile.tsx` is a
 * "use client" module and `lib/wall/tile-liveness.ts` reaches the stream
 * registry, so either import would pull React, the transport and the whole
 * streaming graph into the Playwright process. The wall's unit suite pins this
 * copy; `fleet-execution.spec.ts` mirrors the same strings for the same reason.
 *
 * The cent-rounding matters and is not incidental. The footer renders spend to
 * two decimals, so a run charged less than half a cent CANNOT move it, whatever
 * the ledger says. `spendMoved` asks the server's two figures whether the tile
 * could show the difference at all, so a caller can tell "the tile did not
 * update" apart from "there was nothing a two-decimal figure could show".
 */
import type { Locator, Page } from "@playwright/test";
import { NANOS_PER_USD } from "@/lib/types";

/** Every tile card carries its kind — live, snapshot or drained — as data. The
 * card holds the words a person reads; the overlay link is only how it is
 * found. Mirrors `FleetTile.tsx`'s `data-kind`. */
const TILE_CARD = "[data-kind]";

/** Mirrors of `FleetTile.tsx` / `lib/wall/tile-liveness.ts` copy. */
const MANAGE_FLEET_LABEL = "Manage fleet";
const TILE_SPEND_SUFFIX = "spent";
const TILE_EVENTS_SUFFIX = "events";
const AGENT_PREFIX = "Agent";

/** What the footer prints where the daemon sent no figure at all — never a
 * fabricated `$0.00` or `0`. */
export const TILE_FIGURE_ABSENT = "—";

/** How many decimals the spend cell carries. Mirrors `formatTileSpend`. */
const SPEND_DECIMALS = 2;

/** The two footer figures, as the tile currently prints them. */
export interface TileCounters {
  /** The spend cell verbatim — `$0.03`, or the absent dash. */
  readonly spend: string;
  /** The event count as a number, or `null` for the absent dash. */
  readonly events: number | null;
}

// The VALUE shape is part of each pattern, not a wildcard, and that is
// load-bearing: the tile's own description line reads "wakes on events,
// gathers evidence", so `(\S+)\s+events` captures the word "on" and every
// comparison downstream becomes NaN — a failure that looks like the counter
// never moved. Only a currency figure, a bare count, or the absent dash can
// match, and the LAST match wins so prose can never outrank the footer.
const SPEND_PATTERN = new RegExp(
  `(\\$\\d+\\.\\d{${SPEND_DECIMALS}}|${TILE_FIGURE_ABSENT})\\s+${TILE_SPEND_SUFFIX}\\b`,
  "g",
);
const EVENTS_PATTERN = new RegExp(
  `(\\d+|${TILE_FIGURE_ABSENT})\\s+${TILE_EVENTS_SUFFIX}\\b`,
  "g",
);

/** The last value one pattern matches in the tile's rendered text. */
function figure(rendered: string, pattern: RegExp): string | undefined {
  return [...rendered.matchAll(pattern)].at(-1)?.[1];
}

/**
 * The tile for one agent, found by the callsign in its overlay link's
 * accessible name.
 *
 * By callsign rather than by fleet name: the callsign is derived from the fleet
 * id, so it survives the server suffixing a taken name, and it is the same
 * identifier the Approvals row prints for the same fleet.
 */
export function tileForAgent(page: Page, callsign: string): Locator {
  const overlay = page.getByRole("link", {
    name: new RegExp(`^${MANAGE_FLEET_LABEL}: .+ — ${AGENT_PREFIX} ${callsign} — `),
  });
  return page.locator(TILE_CARD).filter({ has: overlay });
}

/**
 * Both footer figures, read once from whatever the tile is rendering now.
 *
 * A missing figure THROWS rather than answering a placeholder: the tile always
 * renders both cells, so a miss is either a defect or a pattern that stopped
 * matching the copy — and a placeholder would turn both into a comparison that
 * quietly never succeeds.
 */
export async function readTileCounters(tile: Locator): Promise<TileCounters> {
  const rendered = await tile.innerText();
  const spend = figure(rendered, SPEND_PATTERN);
  const events = figure(rendered, EVENTS_PATTERN);
  if (spend === undefined || events === undefined) {
    throw new Error(
      `the tile printed no "${TILE_SPEND_SUFFIX}"/"${TILE_EVENTS_SUFFIX}" figure:\n${rendered}`,
    );
  }
  return {
    spend,
    events: events === TILE_FIGURE_ABSENT ? null : Number(events),
  };
}

/** The spend cell's own rendering of a nanos figure. Mirrors `formatTileSpend`. */
export function renderTileSpend(nanos: number): string {
  return `$${(nanos / NANOS_PER_USD).toFixed(SPEND_DECIMALS)}`;
}

/**
 * Whether a charge is large enough for the cent-rounded spend cell to show it.
 *
 * Asked from the server's before/after figures, never from the rendered ones —
 * the rendered pair is what the caller is trying to grade.
 */
export function spendMoved(beforeNanos: number, afterNanos: number): boolean {
  return renderTileSpend(beforeNanos) !== renderTileSpend(afterNanos);
}
