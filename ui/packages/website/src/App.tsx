import { lazy, Suspense } from "react";
import { Link, NavLink, Route, Routes, ScrollRestoration } from "react-router-dom";
import { Button, WakePulse } from "@usezombie/design-system";
import Home from "./pages/Home";
import Footer from "./components/Footer";
import { APP_BASE_URL, DOCS_URL } from "./config";
import { trackNavigationClicked, trackSignupStarted } from "./analytics/posthog";

/* Secondary routes ship as their own chunks so the landing (/) first-load
 * stays lean. Vite code-splits each React.lazy import by default. */
const Agents = lazy(() => import("./pages/Agents"));
const Privacy = lazy(() => import("./pages/Privacy"));
const Terms = lazy(() => import("./pages/Terms"));
const DesignSystemGallery = lazy(() => import("./pages/DesignSystemGallery"));

const NAV_LINK_CLASS =
  "font-mono text-[12px] uppercase tracking-[0.06em] text-text-muted hover:text-text transition-colors";

export default function App() {
  return (
    <div>
      <ScrollRestoration />

      <header className="topbar">
        <div className="wrap flex items-center justify-between py-4">
          <Link
            to="/"
            className="flex items-center gap-3 font-mono text-[15px] font-medium tracking-[-0.01em] text-text"
            data-testid="brand-link"
          >
            <WakePulse
              live
              data-testid="brand-mark"
              aria-hidden="true"
              className="inline-block size-3 rounded-full bg-pulse"
            />
            <span>usezombie</span>
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

          <Button asChild data-testid="header-install-cta">
            <a
              href={APP_BASE_URL}
              onClick={() =>
                trackSignupStarted({ source: "header_install", surface: "header", mode: "humans" })
              }
            >
              → install
            </a>
          </Button>
        </div>
      </header>

      <main>
        <Suspense fallback={null}>
          <Routes>
            <Route path="/" element={<Home />} />
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
