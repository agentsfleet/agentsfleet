import { describe, expect, it } from "vitest";

import { deriveFleetIdentity } from "./identity";

// The fixture id `FleetTile.test.tsx` pins, and the outputs it pins for it.
// Identity data: operators know their agents by these, so a move of the
// derivation that changed either would rename every agent in their heads.
const PINNED_FLEET_ID = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee";
const PINNED_HASH_SUFFIX = "4bce8453";
const PINNED_CALLSIGN = "Lumen-8453";
const SIGIL_WIDTH = 7;

describe("deriveFleetIdentity, from its home under lib/", () => {
  it("the moved derivation still yields the pinned sigil and callsign", () => {
    // Dimension 5.1.
    const identity = deriveFleetIdentity(PINNED_FLEET_ID);
    expect(identity.hashHex.endsWith(PINNED_HASH_SUFFIX)).toBe(true);
    expect(identity.callsign).toBe(PINNED_CALLSIGN);
  });

  it("mirrors every sigil cell across the centre column", () => {
    const { cells } = deriveFleetIdentity(PINNED_FLEET_ID);
    expect(cells.length).toBeGreaterThan(0);
    for (const cell of cells) {
      expect(cells).toContainEqual({ x: SIGIL_WIDTH - 1 - cell.x, y: cell.y });
    }
  });

  it("is a pure function of the id", () => {
    expect(deriveFleetIdentity("fleet-alpha")).toEqual(deriveFleetIdentity("fleet-alpha"));
    expect(deriveFleetIdentity("fleet-alpha").callsign).not.toBe(
      deriveFleetIdentity("fleet-bravo").callsign,
    );
  });
});
