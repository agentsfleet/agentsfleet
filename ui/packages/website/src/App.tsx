import { Link, NavLink, Route, Routes, ScrollRestoration, useLocation, useNavigate } from "react-router-dom";
import Home from "./pages/Home";
import Pricing from "./pages/Pricing";
import Agents from "./pages/Agents";
import Privacy from "./pages/Privacy";
import Terms from "./pages/Terms";
import Footer from "./components/Footer";
import { Button, AnimatedIcon, ZombieHandIcon } from "@usezombie/design-system";
import { APP_BASE_URL, DOCS_URL } from "./config";

type Mode = "humans" | "agents";

/** Derive mode from current URL — single source of truth. */
function useMode() {
  const location = useLocation();
  const navigate = useNavigate();
  const mode: Mode = location.pathname === "/agents" ? "agents" : "humans";

  function setMode(next: Mode) {
    if (next === "agents") navigate("/agents");
    else navigate("/");
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
            className={mode === "humans" ? "mode-btn active" : "mode-btn"}
            onClick={() => setMode("humans")}
            role="tab"
            aria-selected={mode === "humans"}
            data-testid="mode-humans"
          >
            Humans
          </button>
          <button
            type="button"
            className={mode === "agents" ? "mode-btn active" : "mode-btn"}
            onClick={() => setMode("agents")}
            role="tab"
            aria-selected={mode === "agents"}
            data-testid="mode-agents"
          >
            Agents
          </button>
        </div>

        <nav className="site-nav" aria-label="Primary">
          <NavLink to="/">Home</NavLink>
          <NavLink to="/pricing">Pricing</NavLink>
          <NavLink to="/agents">Agents</NavLink>
          <a href={DOCS_URL} target="_blank" rel="noopener noreferrer">Docs</a>
        </nav>

        <div className="header-actions">
          <Button
            to={APP_BASE_URL}
            className={mode === "humans" ? "header-mission-control z-animated-icon-trigger" : "header-mission-control z-animated-icon-trigger is-hidden"}
            aria-hidden={mode !== "humans"}
            tabIndex={mode === "humans" ? undefined : -1}
          >
            Mission Control{" "}
            <AnimatedIcon trigger="parent-hover" animation="wave"><ZombieHandIcon size={18} /></AnimatedIcon>
          </Button>
        </div>
      </header>

      <main className="site-main">
        <Routes>
          <Route path="/" element={<Home mode={mode} />} />
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
