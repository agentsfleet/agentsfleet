import { lazy, Suspense } from "react";
import { Link, NavLink, Route, Routes, ScrollRestoration } from "react-router-dom";
import { Button, WakePulse } from "@agentsfleet/design-system";
import Home from "./pages/Home";
import Footer from "./components/Footer";
import { DOCS_URL, WAITLIST_URL } from "./config";
import { trackNavigationClicked, trackSignupStarted } from "./analytics/posthog";
import { HERO_PRIMARY_LABEL } from "./lib/marketing-copy";

/* Secondary routes ship as their own chunks so the landing (/) first-load
 * stays lean. Vite code-splits each React.lazy import by default. */
const Fleets = lazy(() => import("./pages/Fleets"));
const Privacy = lazy(() => import("./pages/Privacy"));
const Terms = lazy(() => import("./pages/Terms"));
const About = lazy(() => import("./pages/About"));
const DesignSystemGallery = lazy(() => import("./pages/DesignSystemGallery"));

const NAV_LINK_CLASS =
  "inline-flex min-h-11 min-w-11 items-center justify-center font-sans text-eyebrow uppercase tracking-eyebrow text-text-muted hover:text-text transition-colors";

export default function App() {
  return (
    <div>
      <ScrollRestoration />

      <header className="topbar">
        <div className="wrap flex items-center justify-between py-4">
          <Link
            to="/"
            className="flex min-h-11 items-center gap-3 font-sans text-body font-medium text-text"
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

          <PrimaryNavigation />
          <EarlyAccessAction />
        </div>
      </header>
      <main>
        <Suspense fallback={null}>
          <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/fleets" element={<Fleets />} />
            <Route path="/privacy" element={<Privacy />} />
            <Route path="/terms" element={<Terms />} />
            <Route path="/about" element={<About />} />
            <Route path="/_design-system" element={<DesignSystemGallery />} />
          </Routes>
        </Suspense>
      </main>
      <Footer />
    </div>
  );
}

function PrimaryNavigation() {
  return (
    <nav aria-label="Primary" className="hidden md:flex items-center gap-6">
      <NavLink to="/" end className={NAV_LINK_CLASS}>
        home
      </NavLink>
      <NavLink to="/fleets" className={NAV_LINK_CLASS}>
        fleets
      </NavLink>
      <a href="/#pricing" className={NAV_LINK_CLASS}>
        Early access
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
  );
}

function EarlyAccessAction() {
  return (
    <Button asChild className="min-h-11" data-testid="header-install-cta">
      <a
        href={WAITLIST_URL}
        target="_blank"
        rel="noopener noreferrer"
        onClick={() =>
          trackSignupStarted({
            source: "header_early_access",
            surface: "header",
            mode: "humans",
          })
        }
      >
        → {HERO_PRIMARY_LABEL.toLowerCase()}
      </a>
    </Button>
  );
}
