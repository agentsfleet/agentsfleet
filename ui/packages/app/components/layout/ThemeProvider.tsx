"use client";

import type { ReactNode } from "react";
import { ThemeProvider as NextThemeProvider } from "next-themes";

export function ThemeProvider({children}: {children: ReactNode}) {
  return <NextThemeProvider attribute="data-theme" themes={["dark", "light"]} defaultTheme="dark" enableSystem={false} disableTransitionOnChange>{children}</NextThemeProvider>;
}
