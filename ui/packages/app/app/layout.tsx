import type { Metadata } from "next";
import { cookies } from "next/headers";
import { AuthProvider, AuthSessionKeeper } from "@/lib/auth/client";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";
import AnalyticsBootstrap from "@/components/analytics/AnalyticsBootstrap";
import { THEME_COOKIE, normalizeTheme } from "@/lib/theme";
import { DASHBOARD_ROOT_PATH } from "@/lib/workspace-routes";
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
  // `*FallbackRedirectUrl` (not force) sends a completed sign-in/sign-up to the
  // dashboard root, which resolves the default workspace and redirects to its
  // fleet wall. Fallback (not force) so a legitimate Clerk-internal `redirect_url`
  // (email verification, SSO hops) is still honored.
  return (
    <AuthProvider
      appearance={AUTH_APPEARANCE}
      signInFallbackRedirectUrl={DASHBOARD_ROOT_PATH}
      signUpFallbackRedirectUrl={DASHBOARD_ROOT_PATH}
      localization={{ userButton: { action__manageAccount: "Account" } }}
    >
      <html lang="en" data-theme={theme} suppressHydrationWarning>
        <body>
          <AuthSessionKeeper />
          <AnalyticsBootstrap />
          {/* No tooltip provider here. It belongs to `(dashboard)/layout.tsx`,
              not the root: mounting it above EVERY route group pulled the Radix
              tooltip runtime into the auth bundles — sign-in, sign-up and
              cli-auth each grew 10–20 kB gzipped for a component none of them
              render — and broke the size budget. Every tooltip in the app lives
              under the dashboard. */}
          {children}
        </body>
      </html>
    </AuthProvider>
  );
}
