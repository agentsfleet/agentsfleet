"use client";

import { useEffect } from "react";
import { THEME_COOKIE, THEME_COOKIE_MAX_AGE, DEFAULT_THEME } from "@/lib/theme";

export default function ThemeToggle() {
  // Product default is dark-only now. Keep this tiny client normalizer mounted
  // so stale `theme=light` cookies from earlier builds are overwritten after
  // hydration, while the server stamp already renders dark via normalizeTheme().
  useEffect(() => {
    document.documentElement.dataset.theme = DEFAULT_THEME;
    document.cookie = `${THEME_COOKIE}=${DEFAULT_THEME}; path=/; max-age=${THEME_COOKIE_MAX_AGE}; samesite=lax`;
  }, []);

  return null;
}
