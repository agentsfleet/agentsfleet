export type FleetSigilCell = { x: number; y: number };

export type FleetIdentity = {
  callsign: string;
  hashHex: string;
  cells: FleetSigilCell[];
};

const SIGIL_COLUMNS = 7;
const SIGIL_ROWS = 7;
const SIGIL_HALF_COLUMNS = 4;
const IDENTITY_HASH_OFFSET = 2_166_136_261;
const IDENTITY_HASH_PRIME = 16_777_619;
const CALLSIGN_SUFFIX_LENGTH = 4;
const CALLSIGN_NAME_SHIFT = 16;
const CALLSIGN_NAME_MASK = 31;

// These 32 hash buckets are identity data. Never reorder them; a future
// expansion needs a versioned mapping so existing agents keep their callsigns.
const CALLSIGN_NAMES = [
  "Rivet",
  "Beacon",
  "Bolt",
  "Bumble",
  "Cinder",
  "Comet",
  "Copper",
  "Drift",
  "Echo",
  "Finch",
  "Fizz",
  "Forge",
  "Honey",
  "Kestrel",
  "Lumen",
  "Mica",
  "Moss",
  "Nova",
  "Orbit",
  "Orly",
  "Pixel",
  "Pollen",
  "Quill",
  "Rook",
  "Sable",
  "Scout",
  "Spark",
  "Talon",
  "Tinker",
  "Warden",
  "Willow",
  "Zephyr",
] as const;

function identityHash(seed: string): number {
  let hash = IDENTITY_HASH_OFFSET;
  for (const character of seed) {
    hash ^= character.charCodeAt(0);
    hash = Math.imul(hash, IDENTITY_HASH_PRIME) >>> 0;
  }
  return hash;
}

function sigilCells(hash: number): FleetSigilCell[] {
  const cells: FleetSigilCell[] = [];
  for (let y = 0; y < SIGIL_ROWS; y += 1) {
    for (let x = 0; x < SIGIL_HALF_COLUMNS; x += 1) {
      const bit = y * SIGIL_HALF_COLUMNS + x;
      if (((hash >>> bit) & 1) === 0) continue;
      cells.push({ x, y });
      const mirrorX = SIGIL_COLUMNS - x - 1;
      if (mirrorX !== x) cells.push({ x: mirrorX, y });
    }
  }
  return cells;
}

function callsignForHash(hash: number): string {
  const nameIndex = (hash >>> CALLSIGN_NAME_SHIFT) & CALLSIGN_NAME_MASK;
  // The five-bit mask guarantees an index inside the fixed 32-name table.
  // oxlint-disable-next-line typescript/no-non-null-assertion
  const name = CALLSIGN_NAMES[nameIndex]!;
  const suffix = hash
    .toString(16)
    .slice(-CALLSIGN_SUFFIX_LENGTH)
    .padStart(CALLSIGN_SUFFIX_LENGTH, "0")
    .toUpperCase();
  return `${name}-${suffix}`;
}

export function deriveFleetIdentity(fleetId: string): FleetIdentity {
  const hash = identityHash(fleetId);
  return {
    callsign: callsignForHash(hash),
    hashHex: hash.toString(16),
    cells: sigilCells(hash),
  };
}
