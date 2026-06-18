import { lazy, Suspense } from "react";
import { Link, NavLink, Navigate, Route, Routes, ScrollRestoration } from "react-router-dom";
import { Button, WakePulse } from "@agentsfleet/design-system";
import Home from "./pages/Home";
import Footer from "./components/Footer";
import { DOCS_URL } from "./config";
import { trackNavigationClicked } from "./analytics/posthog";
import { HERO_PRIMARY_LABEL } from "./lib/marketing-copy";

/* Secondary routes ship as their own chunks so the landing (/) first-load
 * stays lean. Vite code-splits each React.lazy import by default. */
const Agents = lazy(() => import("./pages/Agents"));
const Privacy = lazy(() => import("./pages/Privacy"));
const Terms = lazy(() => import("./pages/Terms"));
const DesignSystemGallery = lazy(() => import("./pages/DesignSystemGallery"));

const NAV_LINK_CLASS =
  "inline-flex min-h-11 min-w-11 items-center justify-center font-mono text-eyebrow uppercase tracking-eyebrow text-text-muted hover:text-text transition-colors";

export default function App() {
  return (
    <div>
      <ScrollRestoration />

      <header className="topbar">
        <div className="wrap flex items-center justify-between py-4">
          <Link
            to="/"
            className="flex min-h-11 items-center gap-3 font-mono text-body font-medium text-text"
            data-testid="brand-link"
          >
            <WakePulse
              live
              data-testid="brand-mark"
              aria-hidden="true"
              className="inline-block size-3 rounded-full bg-pulse"
            />
            <span>agentsfleet</span>
          </Link>

          <nav aria-label="Primary" className="hidden md:flex items-center gap-6">
            <NavLink to="/" end className={NAV_LINK_CLASS}>
              home
            </NavLink>
            <NavLink to="/agents" className={NAV_LINK_CLASS}>
              agents
            </NavLink>
            <a href="/#pricing" className={NAV_LINK_CLASS}>
              pricing
            </a>
            <a
              href={DOCS_URL}
              target="_blank"
              rel="noopener noreferrer"
              className={NAV_LINK_CLASS}
              onClick={() =>
                trackNavigationClicked({ source: "header_nav_docs", surface: "header", target: "docs" })
              }
            >
              docs
            </a>
          </nav>

          <Button disabled className="min-h-11" data-testid="header-install-cta">
            → {HERO_PRIMARY_LABEL.toLowerCase()}
          </Button>
        </div>
      </header>

      <main>
        <Suspense fallback={null}>
          <Routes>
            <Route path="/" element={<Home />} />
            {/* /pricing was deleted in favor of the inline /#pricing section.
             * Preserve the old URL for external links + indexed pages with a
             * client-side redirect to the anchor. */}
            <Route path="/pricing" element={<Navigate to="/#pricing" replace />} />
            <Route path="/agents" element={<Agents />} />
            <Route path="/privacy" element={<Privacy />} />
            <Route path="/terms" element={<Terms />} />
            <Route path="/_design-system" element={<DesignSystemGallery />} />
          </Routes>
        </Suspense>
      </main>

      <Footer />
    </div>
  );
}
