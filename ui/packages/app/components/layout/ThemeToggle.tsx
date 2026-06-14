"use client";

import { useEffect, useState } from "react";
import { SunIcon, MoonIcon } from "lucide-react";
import { Button } from "@agentsfleet/design-system";
import { THEME_COOKIE, THEME_COOKIE_MAX_AGE, DEFAULT_THEME, normalizeTheme, type Theme } from "@/lib/theme";

export default function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>(DEFAULT_THEME);

  // Sync from the SSR-stamped (or pre-paint-script-adjusted) attribute on mount
  // so the icon reflects the actual theme rather than the render-time default.
  useEffect(() => {
    setTheme(normalizeTheme(document.documentElement.dataset.theme));
  }, []);

  function toggle() {
    const next: Theme = theme === "light" ? "dark" : "light";
    document.documentElement.dataset.theme = next;
    document.cookie = `${THEME_COOKIE}=${next}; path=/; max-age=${THEME_COOKIE_MAX_AGE}; samesite=lax`;
    setTheme(next);
  }

  const nextLabel = theme === "light" ? "dark" : "light";
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      onClick={toggle}
      aria-label={`Switch to ${nextLabel} theme`}
    >
      {theme === "light" ? <MoonIcon size={16} /> : <SunIcon size={16} />}
    </Button>
  );
}
