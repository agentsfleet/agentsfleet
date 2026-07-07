// Per-user deterministic avatar pattern — a repeating-conic
// "pinwheel" of two hashed hues + a hashed start angle, so two different
// signed-in users get visibly different avatars, not just different colors.
// Pure and deterministic: same seed -> same string, always. This is the one
// sanctioned non-`--pulse` decorative pattern (see docs/DESIGN_SYSTEM.md
// "Sanctioned non-pulse exception") — a static design token can't represent
// a per-user hash, so this is intentionally computed at render time, not
// read from tokens.css/theme.css.
export const AVATAR_GRADIENT_FALLBACK_SEED = "agentsfleet-guest";

function hashToInt(seed: string): number {
  let hash = 0;
  for (let i = 0; i < seed.length; i++) {
    hash = (hash * 31 + seed.charCodeAt(i)) | 0;
  }
  return Math.abs(hash);
}

export function avatarGradient(seed: string): string {
  const resolvedSeed = seed.length > 0 ? seed : AVATAR_GRADIENT_FALLBACK_SEED;
  const hash = hashToInt(resolvedSeed);
  const hue1 = hash % 360;
  const hue2 = (hue1 + 45 + (Math.floor(hash / 360) % 90)) % 360;
  const angle = Math.floor(hash / 129_600) % 360;
  return `repeating-conic-gradient(from ${angle}deg, hsl(${hue1}, 65%, 45%) 0deg 45deg, hsl(${hue2}, 70%, 38%) 45deg 90deg)`;
}
