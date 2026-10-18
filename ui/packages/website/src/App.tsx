import { lazy, Suspense } from "react";
import { Link, Route, Routes, ScrollRestoration, useLocation } from "react-router-dom";
import { BrandMark, Button, Nav } from "@agentsfleet/design-system";
import Home from "./pages/Home";
import Footer from "./components/Footer";
import { PUBLIC_ACCESS_URL, DOCS_URL, GITHUB_URL } from "./config";
import { trackNavigationClicked, trackSignupStarted } from "./analytics/posthog";

/* Secondary routes ship as their own chunks so the landing (/) first-load
 * stays lean. Vite code-splits each React.lazy import by default. */
const Fleets = lazy(() => import("./pages/Fleets"));
const Privacy = lazy(() => import("./pages/Privacy"));
const Terms = lazy(() => import("./pages/Terms"));
const NotFound = lazy(() => import("./pages/NotFound"));
const DesignSystemGallery = import.meta.env.DEV ? lazy(() => import("./pages/DesignSystemGallery")) : null;

const NAV_LINK_CLASS =
  "inline-flex min-h-11 min-w-11 items-center justify-center font-sans text-body-sm text-text-muted hover:text-text transition-colors";

export default function App() {
  const { pathname } = useLocation();
  return (
    <div className={DesignSystemGallery && pathname === "/_design-system" ? undefined : "website-palette"}>
      <a className="skip-link" href="#main-content">Skip to content</a>
      <ScrollRestoration />

      <header className="topbar">
        <div className="wrap flex flex-wrap items-center justify-between gap-4 py-4">
          <Link
            to="/"
            className="flex min-h-11 items-center gap-3 font-sans text-body font-medium text-text"
            data-testid="brand-link"
          >
            <BrandMark data-testid="brand-mark" />
            <span>agentsfleet</span>
          </Link>

          <PrimaryNavigation />
          <DashboardAction />
        </div>
      </header>
      <main id="main-content" tabIndex={-1}>
        <Suspense fallback={null}>
          <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/agents" element={<Fleets />} />
            <Route path="/privacy" element={<Privacy />} />
            <Route path="/terms" element={<Terms />} />
            {DesignSystemGallery ? <Route path="/_design-system" element={<DesignSystemGallery />} /> : null}
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
    <Nav aria-label="Primary" className="primary-navigation order-last flex w-full flex-wrap items-center justify-between gap-x-4 gap-y-2 md:order-none md:w-auto md:gap-6">
      <Link to="/#how-it-works" className={NAV_LINK_CLASS} onClick={() => trackNavigationClicked({ source: "header_nav_product", surface: "header", target: "example" })}>
        Product
      </Link>
      <a href={PUBLIC_ACCESS_URL} target="_blank" rel="noopener noreferrer" className={NAV_LINK_CLASS}
        onClick={() => trackSignupStarted({ source: "header_early_access", surface: "header", mode: "humans" })}>Early access</a>
      <a
        href={DOCS_URL}
        target="_blank"
        rel="noopener noreferrer"
        className={NAV_LINK_CLASS}
        onClick={() =>
          trackNavigationClicked({ source: "header_nav_docs", surface: "header", target: "docs" })
        }
      >
        Docs
      </a>
      <a href={GITHUB_URL} target="_blank" rel="noopener noreferrer" className={NAV_LINK_CLASS}
        onClick={() => trackNavigationClicked({ source: "header_nav_source", surface: "header", target: "github" })}>GitHub ↗</a>
    </Nav>
  );
}

function DashboardAction() {
  return (
    <Button wrap asChild variant="outline" className="min-h-11" data-testid="header-install-cta">
      <a
        href={PUBLIC_ACCESS_URL}
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
        Join waitlist ↗
      </a>
    </Button>
  );
}
