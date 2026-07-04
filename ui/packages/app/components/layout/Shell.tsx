"use client";

import { useEffect, useState } from "react";
import { usePathname } from "next/navigation";
import Link from "next/link";
import {
  LayoutDashboardIcon,
  ActivityIcon,
  SettingsIcon,
  BookOpenIcon,
  BotIcon,
  CheckCircle2Icon,
  CpuIcon,
  LinkIcon,
  CreditCardIcon,
  ServerIcon,
  MenuIcon,
} from "lucide-react";
import {
  Button,
  Dialog,
  DialogContent,
  DialogTitle,
  DialogTrigger,
  EYEBROW_CLASS,
  Nav,
  WakePulse,
} from "@agentsfleet/design-system";
import { cn } from "@/lib/utils";
import { setAnalyticsContext, trackNavigationClicked } from "@/lib/analytics/posthog";
import { SCOPE } from "@/lib/auth/scopes";
import { setActiveWorkspace } from "@/app/(dashboard)/actions";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import WorkspaceSwitcher from "./WorkspaceSwitcher";
import ThemeToggle from "./ThemeToggle";
import ClientOnlyAuthUserButton from "./ClientOnlyAuthUserButton";

type NavEntry = {
  label: string;
  href: string;
  icon: React.ComponentType<{ size?: number }>;
  external?: boolean;
};

// A platform-operator nav entry carries the read scope that reveals it — the
// nav shows a surface iff the session token holds that scope (route_scopes.zig).
type PlatformNavEntry = NavEntry & { scope: string };

const NAV_SURFACE = "app_sidebar";

// Dashboard sits above the labelled groups as a headerless overview entry.
const TOP_NAV: NavEntry[] = [
  { label: "Dashboard", href: "/", icon: LayoutDashboardIcon },
];

// The live work — what the fleets do.
const OPERATIONS_NAV: NavEntry[] = [
  { label: "Fleets", href: "/fleets", icon: BotIcon },
  { label: "Approvals", href: "/approvals", icon: CheckCircle2Icon },
  { label: "Events", href: "/events", icon: ActivityIcon },
];

// What the fleets are wired to — the model brain (which now also hosts the
// write-only secret vault) and the tool connectors, each its own destination;
// plus the execution fleet for platform admins.
const CONFIGURATION_NAV: NavEntry[] = [
  { label: "Models", href: "/settings/models", icon: CpuIcon },
  { label: "Integrations", href: "/integrations", icon: LinkIcon },
];

// Platform-operator surfaces — each appended to the Configuration group only
// when the session token carries that surface's read scope (the backend
// independently gates the routes, so this is discoverability, not the security
// boundary). A runner operator sees Runners; a model operator sees the
// catalogue; a token with neither sees neither.
const PLATFORM_NAV: PlatformNavEntry[] = [
  { label: "Runners", href: "/admin/runners", icon: ServerIcon, scope: SCOPE.RUNNER_READ },
  { label: "Model rates", href: "/admin/models", icon: CpuIcon, scope: SCOPE.MODEL_READ },
];

const ORGANIZATION_NAV: NavEntry[] = [
  { label: "Workspace", href: "/settings", icon: SettingsIcon },
  { label: "Billing", href: "/settings/billing", icon: CreditCardIcon },
];

const BOTTOM_NAV: NavEntry[] = [
  { label: "Docs", href: "https://docs.agentsfleet.net", icon: BookOpenIcon, external: true },
];

// Every internal destination, longest first so a nested route (e.g.
// /settings/models) resolves to its own item rather than its parent Settings.
const INTERNAL_HREFS: string[] = [
  ...TOP_NAV,
  ...OPERATIONS_NAV,
  ...CONFIGURATION_NAV,
  ...PLATFORM_NAV,
  ...ORGANIZATION_NAV,
].map((entry) => entry.href);

function resolveActiveHref(pathname: string): string {
  let active = "";
  for (const href of INTERNAL_HREFS) {
    const hit =
      href === "/" ? pathname === "/" : pathname === href || pathname.startsWith(`${href}/`);
    if (hit && href.length > active.length) active = href;
  }
  return active;
}

function navSource(href: string, label: string, external?: boolean): string {
  if (external) return `${NAV_SURFACE}_${label.toLowerCase()}`;
  return `${NAV_SURFACE}_${href === "/" ? "root" : href.replaceAll("/", "_").replace(/^_+/, "")}`;
}

type ShellProps = {
  children: React.ReactNode;
  workspaces?: TenantWorkspace[];
  activeWorkspaceId?: string | null;
  /** Operator scopes on the session token; gate the platform nav per-surface. */
  operatorScopes?: string[];
};

export default function Shell({
  children,
  workspaces = [],
  activeWorkspaceId = null,
  operatorScopes = [],
}: ShellProps) {
  const pathname = usePathname();
  const activeHref = resolveActiveHref(pathname);
  const isActive = (href: string) => href === activeHref;

  // Bind the active workspace as the PostHog group + record workspace_count on
  // the person, so every event/pageview is sliceable per workspace (Supabase
  // group-analytics model). Best-effort + queued until posthog-js loads.
  useEffect(() => {
    setAnalyticsContext({
      workspaceId: activeWorkspaceId,
      workspaceCount: workspaces.length,
    });
  }, [activeWorkspaceId, workspaces.length]);

  return (
    <div className="app-glow-surface grid min-h-screen md:grid-cols-[240px_1fr] grid-rows-[56px_1fr]" data-glow="dashboard">
      <header className="col-span-full sticky top-0 z-40 flex items-center gap-4 px-4 md:px-6 border-b border-border bg-background/85 backdrop-blur">
        <MobileNav isActive={isActive} operatorScopes={operatorScopes} />

        <Link
          href="/"
          className="inline-flex items-center gap-2 font-mono text-sm font-medium tracking-tight text-foreground no-underline"
          aria-label="agentsfleet home"
        >
          <WakePulse
            live
            className="inline-block w-3 h-3 rounded-full bg-pulse"
            aria-hidden="true"
          />
          <span>agentsfleet</span>
        </Link>

        <div className="flex-1" />

        <WorkspaceSwitcher
          workspaces={workspaces}
          activeId={activeWorkspaceId}
          onSwitch={setActiveWorkspace}
        />

        <ThemeToggle />

        <ClientOnlyAuthUserButton />
      </header>

      <aside className="hidden md:flex flex-col bg-muted border-r border-border sticky top-14 h-[calc(100vh-56px)] overflow-y-auto py-4">
        <SidebarNav isActive={isActive} onNavigate={() => {}} operatorScopes={operatorScopes} />
      </aside>

      <main className="app-dashboard-canvas overflow-auto px-4 py-6 sm:px-6 md:px-8 md:py-8 2xl:px-12">
        {/* Full-width canvas: the page spans the available width at every
         * breakpoint, with gutters that grow on large screens. Long-form text /
         * forms cap themselves with a per-component measure, and short content
         * centres rather than stretching. */}
        <div className="w-full">{children}</div>
      </main>
    </div>
  );
}

function MobileNav({
  isActive,
  operatorScopes,
}: {
  isActive: (href: string) => boolean;
  operatorScopes: string[];
}) {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button
          type="button"
          aria-label="Open navigation"
          variant="ghost"
          size="icon"
          className="md:hidden -ml-2"
        >
          <MenuIcon size={18} />
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-xs">
        <DialogTitle className="sr-only">Navigation</DialogTitle>
        <SidebarNav isActive={isActive} onNavigate={() => setOpen(false)} operatorScopes={operatorScopes} />
      </DialogContent>
    </Dialog>
  );
}

type NavProps = {
  isActive: (href: string) => boolean;
  onNavigate: () => void;
  operatorScopes: string[];
};

function SidebarNav({ isActive, onNavigate, operatorScopes }: NavProps) {
  // Each platform surface appears iff the session token holds its read scope;
  // a token with neither scope sees the plain Configuration group.
  const platformItems = PLATFORM_NAV.filter((entry) => operatorScopes.includes(entry.scope));
  const configItems = [...CONFIGURATION_NAV, ...platformItems];
  return (
    <Nav aria-label="Primary" className="flex flex-col h-full">
      <NavSection items={TOP_NAV} isActive={isActive} onNavigate={onNavigate} />
      <NavSection label="Automations" items={OPERATIONS_NAV} isActive={isActive} onNavigate={onNavigate} />
      <NavSection label="Configuration" items={configItems} isActive={isActive} onNavigate={onNavigate} />
      <NavSection label="Organization" items={ORGANIZATION_NAV} isActive={isActive} onNavigate={onNavigate} />
      <div className="mt-auto">
        <NavSection items={BOTTOM_NAV} isActive={isActive} onNavigate={onNavigate} />
      </div>
    </Nav>
  );
}

function NavSection({
  label,
  items,
  isActive,
  onNavigate,
}: {
  label?: string;
  items: NavEntry[];
  isActive: (href: string) => boolean;
  onNavigate: () => void;
}) {
  return (
    <NavGroup label={label}>
      {items.map(({ label: itemLabel, href, icon: Icon, external }) => (
        <NavItem
          key={href}
          href={href}
          label={itemLabel}
          Icon={Icon}
          external={external}
          active={external ? false : isActive(href)}
          onClick={() => {
            onNavigate();
            trackNavigationClicked({
              source: navSource(href, itemLabel, external),
              surface: NAV_SURFACE,
              target: href,
            });
          }}
        />
      ))}
    </NavGroup>
  );
}

function NavGroup({ label, children }: { label?: string; children: React.ReactNode }) {
  return (
    <div className="px-3 mb-6">
      {label ? (
        <div className={cn(EYEBROW_CLASS, "text-muted-foreground px-2 mb-2")}>
          {label}
        </div>
      ) : null}
      <div className="flex flex-col gap-0.5">{children}</div>
    </div>
  );
}

type NavItemProps = {
  href: string;
  label: string;
  Icon: React.ComponentType<{ size?: number }>;
  active?: boolean;
  external?: boolean;
  onClick?: () => void;
};

// `transition` (not just -colors) so the motion-safe hover nudge animates;
// `motion-safe:` drops the nudge entirely under prefers-reduced-motion.
// text-body-sm (13px), not text-eyebrow (12px, section-label scale) — a nav
// link is primary interactive content, so it must render a step above the
// group header labelling it, never level with or under it.
const NAV_ITEM_CLASSES =
  "flex items-center gap-2.5 px-3 py-2 rounded-md font-mono text-body-sm text-muted-foreground no-underline transition duration-snap ease-snap motion-safe:hover:translate-x-px hover:bg-accent hover:text-foreground data-[active=true]:bg-accent data-[active=true]:text-foreground";

function NavItem({ href, label, Icon, active, external, onClick }: NavItemProps) {
  if (external) {
    return (
      <a
        href={href}
        target="_blank"
        rel="noopener noreferrer"
        className={NAV_ITEM_CLASSES}
        onClick={onClick}
      >
        <Icon size={15} />
        {label}
      </a>
    );
  }
  return (
    <Link
      href={href}
      data-active={active ? "true" : undefined}
      className={NAV_ITEM_CLASSES}
      onClick={onClick}
    >
      <Icon size={15} />
      {label}
    </Link>
  );
}
