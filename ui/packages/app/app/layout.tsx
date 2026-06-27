import type { Metadata } from "next";
import { cookies } from "next/headers";
import { AuthProvider } from "@/lib/auth/client";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";
import AnalyticsBootstrap from "@/components/analytics/AnalyticsBootstrap";
import { THEME_COOKIE, normalizeTheme } from "@/lib/theme";
import "./globals.css";

export const metadata: Metadata = {
  title: "agentsfleet — Dashboard",
  description: "Fleet delivery control plane. Manage workspaces, runs, and pipeline visibility.",
};

// Dark is the product surface. The SSR stamp below still reads the cookie, but
// normalizeTheme() maps every value to dark so stale light cookies from older
// builds cannot repaint auth or dashboard screens.
export default async function RootLayout({ children }: { children: React.ReactNode }) {
  const theme = normalizeTheme((await cookies()).get(THEME_COOKIE)?.value);
  return (
    <AuthProvider appearance={AUTH_APPEARANCE} localization={{ userButton: { action__manageAccount: "Account" } }}>
      <html lang="en" data-theme={theme} suppressHydrationWarning>
        <body>
          <AnalyticsBootstrap />
          {children}
        </body>
      </html>
    </AuthProvider>
  );
}
