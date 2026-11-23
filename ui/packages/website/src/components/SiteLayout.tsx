import { Suspense, type ReactNode } from "react";
import { SiteFooter, SiteHeader } from "./SiteNavigation";

export function SiteLayout({
  waitlistUrl,
  landing = false,
  children,
}: {
  waitlistUrl: string;
  landing?: boolean;
  children: ReactNode;
}) {
  return (
    <Suspense fallback={null}>
      <div className="flex min-h-svh flex-col bg-background font-sans text-body leading-body text-text antialiased [overflow-wrap:anywhere]">
        <a
          className="sr-only z-50 rounded-md bg-cta p-lg text-cta-foreground focus:not-sr-only focus:fixed focus:top-xl focus:left-xl"
          href="#main-content"
        >
          Skip to content
        </a>
        <SiteHeader waitlistUrl={waitlistUrl} landing={landing} />
        <main id="main-content" className="min-w-0 grow" tabIndex={-1}>
          {children}
        </main>
        <SiteFooter waitlistUrl={waitlistUrl} />
      </div>
    </Suspense>
  );
}
