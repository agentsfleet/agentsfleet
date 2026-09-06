import { lazy, Suspense } from "react";
import { Link, NavLink, Navigate, Route, Routes, ScrollRestoration } from "react-router-dom";
import { Button, WakePulse } from "@agentsfleet/design-system";
import Home from "./pages/Home";
import Footer from "./components/Footer";
import { APP_BASE_URL, DOCS_URL } from "./config";
import { trackNavigationClicked } from "./analytics/posthog";

/* Secondary routes ship as their own chunks so the landing (/) first-load
 * stays lean. Vite code-splits each React.lazy import by default. */
const Fleets = lazy(() => import("./pages/Fleets"));
const Privacy = lazy(() => import("./pages/Privacy"));
const Terms = lazy(() => import("./pages/Terms"));
const NotFound = lazy(() => import("./pages/NotFound"));
const DesignSystemGallery = lazy(() => import("./pages/DesignSystemGallery"));

const NAV_LINK_CLASS =
  "inline-flex min-h-11 min-w-11 items-center justify-center font-sans text-body-sm text-text-muted hover:text-text transition-colors";

export default function App() {
  return (
    <div>
      <ScrollRestoration />

      <header className="topbar">
        <div className="wrap flex flex-wrap items-center justify-between gap-4 py-4">
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
          <DashboardAction />
        </div>
      </header>
      <main>
        <Suspense fallback={null}>
          <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/pricing" element={<Navigate to="/#pricing" replace />} />
            <Route path="/fleets" element={<Navigate to="/agents" replace />} />
            <Route path="/agents" element={<Fleets />} />
            <Route path="/privacy" element={<Privacy />} />
            <Route path="/terms" element={<Terms />} />
            <Route path="/_design-system" element={<DesignSystemGallery />} />
            <Route path="*" element={<NotFound />} />
          </Routes>
        </Suspense>
      </main>
      <Footer />
    </div>
  );
}

function PrimaryNavigation() {
  return (
    <nav aria-label="Primary" className="primary-navigation order-last flex w-full flex-wrap items-center justify-between gap-x-4 gap-y-2 md:order-none md:w-auto md:gap-6">
      <NavLink to="/" end className={NAV_LINK_CLASS}>
        home
      </NavLink>
      <NavLink to="/agents" className={NAV_LINK_CLASS}>
        agents
      </NavLink>
      <Link to="/#how-it-works" className={NAV_LINK_CLASS}>
        how it works
      </Link>
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

function DashboardAction() {
  return (
    <Button wrap asChild className="min-h-11" data-testid="header-install-cta">
      <a
        href={APP_BASE_URL}
        target="_blank"
        rel="noopener noreferrer"
        onClick={() =>
          trackNavigationClicked({
            source: "header_dashboard",
            surface: "header",
            target: "dashboard",
          })
        }
      >
        dashboard
      </a>
    </Button>
  );
}
