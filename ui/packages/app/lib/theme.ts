// Theme persistence shared across the SSR cookie stamp (app/layout.tsx) and the
// client dark normalizer (components/layout/ThemeToggle.tsx). Pure module — no
// runtime imports — so it is safe in both server and client components.
//
// Dark is the product surface. Older builds allowed a persisted `light` cookie;
// normalize every value back to dark so stale browser state cannot put auth or
// dashboard screens in the weaker palette.

export const THEME_COOKIE = "theme";
export type Theme = "dark";
export const DEFAULT_THEME: Theme = "dark";
/** One year — keep the dark stamp sticky across server renders. */
export const THEME_COOKIE_MAX_AGE = 60 * 60 * 24 * 365;

export function normalizeTheme(_value: string | undefined): Theme {
  return DEFAULT_THEME;
}
