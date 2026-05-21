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

// Pre-paint theme init: the SSR-stamped data-theme below already reflects a
// saved cookie. This only covers the cookie-less first visit — fall back to the
// OS preference before hydration (prefers-color-scheme can't be read
// server-side). suppressHydrationWarning on <html> permits the attribute write.
const THEME_INIT_SCRIPT = `try{var m=document.cookie.match(/(?:^|; )${THEME_COOKIE}=(light|dark)/);if(m){document.documentElement.dataset.theme=m[1];}else if(window.matchMedia&&window.matchMedia('(prefers-color-scheme: light)').matches){document.documentElement.dataset.theme='light';}}catch(e){}`;

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  const theme = normalizeTheme((await cookies()).get(THEME_COOKIE)?.value);
  return (
    <AuthProvider>
      <html lang="en" data-theme={theme} suppressHydrationWarning>
        <body>
          <script dangerouslySetInnerHTML={{ __html: THEME_INIT_SCRIPT }} />
          <AnalyticsBootstrap />
          {children}
        </body>
      </html>
    </AuthProvider>
  );
}
