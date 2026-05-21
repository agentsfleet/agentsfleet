// Theme persistence shared across the SSR cookie stamp (app/layout.tsx) and the
// client toggle (components/layout/ThemeToggle.tsx). Pure module — no runtime
// imports — so it is safe in both server and client components.
//
// Dark is the brand default (design-system tokens.css `:root`); the only other
// palette is `[data-theme="light"]`, so the toggle is binary.

export const THEME_COOKIE = "theme";
export type Theme = "light" | "dark";
export const DEFAULT_THEME: Theme = "dark";
/** One year — the preference is sticky until the user flips it again. */
export const THEME_COOKIE_MAX_AGE = 60 * 60 * 24 * 365;

export function normalizeTheme(value: string | undefined): Theme {
  return value === "light" ? "light" : DEFAULT_THEME;
}
