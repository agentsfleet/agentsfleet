"use client";

import { usePathname } from "next/navigation";
import Link from "next/link";
import { AuthUserButton } from "@/lib/auth/client";
import { trackNavigationClicked } from "@/lib/analytics/posthog";
import WorkspaceSwitcher from "./WorkspaceSwitcher";
import { setActiveWorkspace } from "@/app/(dashboard)/actions";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import {
  LayoutDashboardIcon,
  ActivityIcon,
  SettingsIcon,
  BookOpenIcon,
  ZapIcon,
  SkullIcon,
  KeyRoundIcon,
  CheckCircle2Icon,
} from "lucide-react";

const NAV = [
  {
    label: "Dashboard",
    href: "/",
    icon: LayoutDashboardIcon,
  },
  {
    label: "Zombies",
    href: "/zombies",
    icon: SkullIcon,
  },
  {
    label: "Credentials",
    href: "/credentials",
    icon: KeyRoundIcon,
  },
  {
    label: "Approvals",
    href: "/approvals",
    icon: CheckCircle2Icon,
  },
  {
    label: "Events",
    href: "/events",
    icon: ActivityIcon,
  },
];

const BOTTOM_NAV = [
  {
    label: "Docs",
    href: "https://docs.usezombie.com",
    icon: BookOpenIcon,
    external: true,
  },
  {
    label: "Settings",
    href: "/settings",
    icon: SettingsIcon,
  },
];

type ShellProps = {
  children: React.ReactNode;
  workspaces?: TenantWorkspace[];
  activeWorkspaceId?: string | null;
};

export default function Shell({ children, workspaces = [], activeWorkspaceId = null }: ShellProps) {
  const pathname = usePathname();

  function isActive(href: string) {
    if (href === "/") return pathname === "/";
    return pathname.startsWith(href);
  }

  return (
    <div className="mc-shell">
      {/* Header */}
      <header className="mc-header">
        <Link href="/" className="mc-brand">
          <ZapIcon size={16} className="mc-brand-icon" />
          <span>UseZombie</span>
          <span className="mc-brand-tag">Mission Control</span>
        </Link>

        <div style={{ flex: 1 }} />

        <WorkspaceSwitcher
          workspaces={workspaces}
          activeId={activeWorkspaceId}
          onSwitch={setActiveWorkspace}
        />

        <nav className="mc-header-nav">
          <a
            href="https://docs.usezombie.com"
            target="_blank"
            rel="noopener noreferrer"
            className="mc-header-link"
            onClick={() => trackNavigationClicked({ source: "app_header_docs", surface: "app_header", target: "docs" })}
          >
            Docs
          </a>
          <a
            href="https://usezombie.com"
            target="_blank"
            rel="noopener noreferrer"
            className="mc-header-link"
            onClick={() => trackNavigationClicked({ source: "app_header_marketing", surface: "app_header", target: "marketing_site" })}
          >
            UseZombie.com
          </a>
        </nav>

        <AuthUserButton
          appearance={{
            variables: {
              colorPrimary: "var(--z-orange)",
              colorBackground: "#0f1520",
              colorText: "#e8f2ff",
              borderRadius: "8px",
            },
          }}
        />
      </header>

      {/* Sidebar */}
      <aside className="mc-sidebar">
        <div className="mc-nav-section">
          <div className="mc-nav-label">Navigation</div>
          {NAV.map(({ label, href, icon: Icon }) => (
            <Link
              key={href}
              href={href}
              className={`mc-nav-item${isActive(href) ? " active" : ""}`}
              onClick={() =>
                trackNavigationClicked({
                  source: `app_sidebar_${href === "/" ? "root" : href.replaceAll("/", "_").replace(/^_+/, "")}`,
                  surface: "app_sidebar",
                  target: href,
                })
              }
            >
              <Icon size={15} />
              {label}
            </Link>
          ))}
        </div>

        <div className="mc-nav-section" style={{ marginTop: "auto" }}>
          <div className="mc-nav-label">More</div>
          {BOTTOM_NAV.map(({ label, href, icon: Icon, external }) => (
            <a
              key={href}
              href={href}
              className="mc-nav-item"
              {...(external ? { target: "_blank", rel: "noopener noreferrer" } : {})}
              onClick={() =>
                trackNavigationClicked({
                  source: `app_sidebar_more_${label.toLowerCase().replace(/\s+/g, "_")}`,
                  surface: "app_sidebar_more",
                  target: href,
                })
              }
            >
              <Icon size={15} />
              {label}
            </a>
          ))}
        </div>
      </aside>

      {/* Main content */}
      <main className="mc-content">{children}</main>

      <style>{`
        .mc-brand {
          display: flex;
          align-items: center;
          gap: 0.5rem;
          text-decoration: none;
          font-weight: 700;
          font-size: 1rem;
          letter-spacing: 0.02em;
        }
        .mc-brand-icon { color: var(--z-orange); }
        .mc-brand-tag {
          font-family: var(--z-font-mono);
          font-size: 0.68rem;
          color: var(--z-amber);
          text-transform: uppercase;
          letter-spacing: 0.08em;
          padding: 0.15rem 0.45rem;
          border: 1px solid rgba(255, 190, 46, 0.2);
          border-radius: var(--z-radius-pill);
        }
        .mc-header-nav {
          display: flex;
          gap: 1rem;
          margin-right: 1rem;
        }
        .mc-header-link {
          font-size: 0.82rem;
          color: var(--z-text-muted);
          text-decoration: none;
          transition: color 0.15s;
        }
        .mc-header-link:hover { color: var(--z-text-primary); }
      `}</style>
    </div>
  );
}
