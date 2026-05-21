import type { Metadata } from "next";
import { cookies } from "next/headers";
import { AuthProvider } from "@/lib/auth/client";
import AnalyticsBootstrap from "@/components/analytics/AnalyticsBootstrap";
import { THEME_COOKIE, normalizeTheme } from "@/lib/theme";
import "./globals.css";

export const metadata: Metadata = {
  title: "usezombie — Dashboard",
  description: "Agent delivery control plane. Manage workspaces, runs, and pipeline visibility.",
};

// Dark is the brand default; the only other palette is `[data-theme="light"]`.
// The SSR stamp below reads the saved cookie (absent → dark), so the server
// renders the correct palette with no flash and no client-side re-paint. The
// header toggle flips `data-theme` + writes the cookie; the next SSR load
// re-stamps from it. Auth pages stay dark because a logged-out first visit has
// no cookie — we deliberately do NOT auto-switch to the OS light preference.
export default async function RootLayout({ children }: { children: React.ReactNode }) {
  const theme = normalizeTheme((await cookies()).get(THEME_COOKIE)?.value);
  return (
    <AuthProvider>
      <html lang="en" data-theme={theme} suppressHydrationWarning>
        <body>
          <AnalyticsBootstrap />
          {children}
        </body>
      </html>
    </AuthProvider>
  );
}
