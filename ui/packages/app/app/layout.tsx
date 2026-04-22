import type { Metadata } from "next";
import { AuthProvider } from "@/lib/auth/client";
import AnalyticsBootstrap from "@/components/analytics/AnalyticsBootstrap";
import "./globals.css";

export const metadata: Metadata = {
  title: "UseZombie — Mission Control",
  description: "Agent delivery control plane. Manage workspaces, runs, and pipeline visibility.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <AuthProvider>
      <html lang="en" suppressHydrationWarning>
        <body>
          <AnalyticsBootstrap />
          {children}
        </body>
      </html>
    </AuthProvider>
  );
}
