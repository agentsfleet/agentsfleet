export const AVATAR_COLOR_FALLBACK_SEED = "agentsfleet-guest";
const HASH_MULTIPLIER = 31;
const HUE_DEGREES = 360;

function hashToInt(seed: string): number {
  let hash = 0;
  for (let i = 0; i < seed.length; i++) {
    hash = (hash * HASH_MULTIPLIER + seed.charCodeAt(i)) | 0;
  }
  return Math.abs(hash);
}

export function avatarColor(seed: string): string {
  const resolvedSeed = seed.length > 0 ? seed : AVATAR_COLOR_FALLBACK_SEED;
  const hash = hashToInt(resolvedSeed);
  return `hsl(${hash % HUE_DEGREES}, 35%, 28%)`;
}
