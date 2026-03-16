import { Link, NavLink, Route, Routes, ScrollRestoration, useLocation, useNavigate } from "react-router-dom";
import Home from "./pages/Home";
import Pricing from "./pages/Pricing";
import Agents from "./pages/Agents";
import Privacy from "./pages/Privacy";
import Terms from "./pages/Terms";
import Footer from "./components/Footer";
import { Button, AnimatedIcon, ZombieHandIcon } from "@usezombie/design-system";
import { APP_BASE_URL, DOCS_URL } from "./config";
import { trackNavigationClicked, trackSignupStarted } from "./analytics/posthog";
import { getModeFromPathname, MODE_AGENTS, MODE_HUMANS, MODE_PATHS, type Mode } from "./constants/mode";

/** Derive mode from current URL — single source of truth. */
function useMode() {
  const location = useLocation();
  const navigate = useNavigate();
  const mode: Mode = getModeFromPathname(location.pathname);

  function setMode(next: Mode) {
    navigate(MODE_PATHS[next]);
  }

  return [mode, setMode] as const;
}

/** Floating particles for animated background. */
function ParticleField() {
  const particles = Array.from({ length: 12 }, (_, i) => ({
    key: i,
    left: `${(i * 8.3) % 100}%`,
    dur: `${10 + (i % 5) * 3}s`,
    delay: `${(i * 1.7) % 8}s`,
    bottom: `-${5 + (i % 4) * 3}%`,
  }));

  return (
    <div className="particle-field" aria-hidden="true">
      {particles.map((p) => (
        <div
          key={p.key}
          className="p"
          style={{
            left: p.left,
            bottom: p.bottom,
            ["--dur" as string]: p.dur,
            ["--delay" as string]: p.delay,
          }}
        />
      ))}
    </div>
  );
}

export default function App() {
  const [mode, setMode] = useMode();
  const isHumansMode = mode === MODE_HUMANS;
  const isAgentsMode = mode === MODE_AGENTS;

  return (
    <div className="site-shell">
      <ParticleField />

      <header className="site-header">
        <ScrollRestoration />
        <div className="brand-wrap">
          <Link className="brand" to="/">
            UseZombie
          </Link>
          <span className="badge">agent delivery control plane</span>
        </div>

        <div className="mode-switch" role="tablist" aria-label="Mode switch" data-testid="mode-switch">
          <button
            type="button"
            className={isHumansMode ? "mode-btn active" : "mode-btn"}
            onClick={() => setMode(MODE_HUMANS)}
            role="tab"
            aria-selected={isHumansMode}
            data-testid="mode-humans"
          >
            Humans
          </button>
          <button
            type="button"
            className={isAgentsMode ? "mode-btn active" : "mode-btn"}
            onClick={() => setMode(MODE_AGENTS)}
            role="tab"
            aria-selected={isAgentsMode}
            data-testid="mode-agents"
          >
            Agents
          </button>
        </div>

        <nav className="site-nav" aria-label="Primary">
          <NavLink to="/">Home</NavLink>
          <NavLink to="/pricing">Pricing</NavLink>
          <NavLink to="/agents">Agents</NavLink>
          <a
            href={DOCS_URL}
            target="_blank"
            rel="noopener noreferrer"
            onClick={() => trackNavigationClicked({ source: "header_nav_docs", surface: "header", target: "docs" })}
          >
            Docs
          </a>
        </nav>

        <div className="header-actions">
          <Button
            to={APP_BASE_URL}
            onClick={() => trackSignupStarted({ source: "header_mission_control", surface: "header", mode })}
            className={isHumansMode ? "header-mission-control z-animated-icon-trigger" : "header-mission-control z-animated-icon-trigger is-hidden"}
            aria-hidden={!isHumansMode}
            tabIndex={isHumansMode ? undefined : -1}
          >
            <span>Mission Control</span>
            <span className="header-mission-control-icon" aria-hidden="true">
              <AnimatedIcon trigger="parent-hover" animation="wiggle"><ZombieHandIcon size={18} /></AnimatedIcon>
            </span>
          </Button>
        </div>
      </header>

      <main className="site-main">
        <Routes>
          <Route path="/" element={<Home />} />
          <Route path="/pricing" element={<Pricing />} />
          <Route path="/agents" element={<Agents />} />
          <Route path="/privacy" element={<Privacy />} />
          <Route path="/terms" element={<Terms />} />
        </Routes>
      </main>

      <Footer />
    </div>
  );
}
