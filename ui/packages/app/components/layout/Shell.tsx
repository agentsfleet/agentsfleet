"use client";

import { useEffect, useState } from "react";
import { usePathname } from "next/navigation";
import Link from "next/link";
import {
  LayoutDashboardIcon,
  ActivityIcon,
  BookOpenIcon,
  BotIcon,
  CheckCircle2Icon,
  CpuIcon,
  CoinsIcon,
  KeyIcon,
  KeyRoundIcon,
  LinkIcon,
  CreditCardIcon,
  ServerIcon,
  MenuIcon,
  PanelLeftIcon,
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

// aria-controls target for the collapse toggle — ties it to the <aside> it collapses.
// The two pixel widths the toggle drives live only as literal Tailwind
// arbitrary-value strings ("md:grid-cols-[64px_1fr]" / "[240px_1fr]") below —
// Tailwind statically scans source for class-name literals, so a variable
// can't be interpolated into one at runtime.
const SIDEBAR_NAV_ID = "app-sidebar-nav";

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
  { label: "Secrets & ENVs", href: "/secrets", icon: KeyRoundIcon },
];

// Platform-operator surfaces — each appended to the Configuration group only
// when the session token carries that surface's read scope (the backend
// independently gates the routes, so this is discoverability, not the security
// boundary). A runner operator sees Runners; a model operator sees the
// catalogue; a token with neither sees neither.
const PLATFORM_NAV: PlatformNavEntry[] = [
  { label: "Runners", href: "/admin/runners", icon: ServerIcon, scope: SCOPE.RUNNER_READ },
  { label: "Model rates", href: "/admin/models", icon: CoinsIcon, scope: SCOPE.MODEL_READ },
];

const ORGANIZATION_NAV: NavEntry[] = [
  { label: "API Keys", href: "/settings/api-keys", icon: KeyIcon },
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
  // Not persisted — every dashboard load starts expanded, matching the
  // reference product's behavior (Supabase Studio's sidebar also resets on
  // reload rather than remembering a per-user preference).
  const [collapsed, setCollapsed] = useState(false);

  // Bind the active workspace as the PostHog group + record workspace_count on
  // the person, so every event/pageview is sliceable per workspace (Supabase
  // group-analytics model). Best-effort + queued until posthog-js loads.
  useEffect(() => {
    setAnalyticsContext({
      workspaceId: activeWorkspaceId,
      workspaceCount: workspaces.length,
    });
  }, [activeWorkspaceId, workspaces.length]);

  function toggleCollapsed() {
    setCollapsed((prev) => !prev);
  }

  return (
    <div
      className={cn(
        "app-glow-surface grid min-h-screen grid-rows-[56px_1fr]",
        collapsed ? "md:grid-cols-[64px_1fr]" : "md:grid-cols-[240px_1fr]",
      )}
      data-glow="dashboard"
    >
      <header className="col-span-full sticky top-0 z-40 flex items-center gap-4 px-4 md:px-6 border-b border-border bg-background/85 backdrop-blur">
        <MobileNav isActive={isActive} operatorScopes={operatorScopes} />

        <Button
          type="button"
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          aria-expanded={!collapsed}
          aria-controls={SIDEBAR_NAV_ID}
          variant="ghost"
          size="icon"
          className="hidden md:inline-flex -ml-2"
          onClick={toggleCollapsed}
        >
          <PanelLeftIcon size={18} />
        </Button>

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

      <aside
        id={SIDEBAR_NAV_ID}
        className="hidden md:flex flex-col bg-muted border-r border-border sticky top-14 h-[calc(100vh-56px)] overflow-y-auto py-4"
      >
        <SidebarNav
          isActive={isActive}
          onNavigate={() => {}}
          operatorScopes={operatorScopes}
          collapsed={collapsed}
        />
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
        {/* The mobile dialog always renders expanded — there's no width
         * constraint driving a collapse here, and hiding labels in a picker
         * the user just opened to find something would be counterproductive. */}
        <SidebarNav isActive={isActive} onNavigate={() => setOpen(false)} operatorScopes={operatorScopes} collapsed={false} />
      </DialogContent>
    </Dialog>
  );
}

type NavProps = {
  isActive: (href: string) => boolean;
  onNavigate: () => void;
  operatorScopes: string[];
  collapsed: boolean;
};

function SidebarNav({ isActive, onNavigate, operatorScopes, collapsed }: NavProps) {
  // Each platform surface appears iff the session token holds its read scope;
  // a token with neither scope sees the plain Configuration group.
  const platformItems = PLATFORM_NAV.filter((entry) => operatorScopes.includes(entry.scope));
  const configItems = [...CONFIGURATION_NAV, ...platformItems];
  return (
    <Nav aria-label="Primary" className="flex flex-col h-full">
      <NavSection items={TOP_NAV} isActive={isActive} onNavigate={onNavigate} collapsed={collapsed} />
      <NavSection label="Automations" items={OPERATIONS_NAV} isActive={isActive} onNavigate={onNavigate} collapsed={collapsed} />
      <NavSection label="Configuration" items={configItems} isActive={isActive} onNavigate={onNavigate} collapsed={collapsed} />
      <NavSection label="Organization" items={ORGANIZATION_NAV} isActive={isActive} onNavigate={onNavigate} collapsed={collapsed} />
      <div className="mt-auto">
        <NavSection items={BOTTOM_NAV} isActive={isActive} onNavigate={onNavigate} collapsed={collapsed} />
      </div>
    </Nav>
  );
}

function NavSection({
  label,
  items,
  isActive,
  onNavigate,
  collapsed,
}: {
  label?: string;
  items: NavEntry[];
  isActive: (href: string) => boolean;
  onNavigate: () => void;
  collapsed: boolean;
}) {
  return (
    <NavGroup label={label} collapsed={collapsed}>
      {items.map(({ label: itemLabel, href, icon: Icon, external }) => (
        <NavItem
          key={href}
          href={href}
          label={itemLabel}
          Icon={Icon}
          external={external}
          active={external ? false : isActive(href)}
          collapsed={collapsed}
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

function NavGroup({
  label,
  collapsed,
  children,
}: {
  label?: string;
  collapsed: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className="px-3 mb-6">
      {label && !collapsed ? (
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
  collapsed?: boolean;
  onClick?: () => void;
};

// `transition` (not just -colors) so the motion-safe hover nudge animates;
// `motion-safe:` drops the nudge entirely under prefers-reduced-motion.
// text-body-sm (13px), not text-eyebrow (12px, section-label scale) — a nav
// link is primary interactive content, so it must render a step above the
// group header labelling it, never level with or under it.
// Active state uses the pulse/mint token, not the generic accent surface —
// docs/DESIGN_SYSTEM.md and tokens.css both reserve mint for "accents / links
// / active / glow"; this is the one spot in the nav that's actually meant to
// claim it. `bg-pulse/10` mirrors the same opacity-modifier pattern Alert's
// `success` variant already uses for a soft (not solid) status fill. The left
// accent bar mirrors tab-styles.ts's `border-b-2 … data-[active=true]:border-pulse`
// pattern so the fill isn't the only active signal. `rounded-r-md` (not
// `rounded-md`) — rounding the left corners too would curve the accent bar's
// top/bottom ends instead of a crisp straight vertical line flush against
// the sidebar edge.
const NAV_ITEM_CLASSES =
  "flex items-center gap-2.5 px-3 py-2 rounded-r-md border-l-2 border-transparent font-mono text-body-sm text-muted-foreground no-underline transition duration-snap ease-snap motion-safe:hover:translate-x-px hover:bg-accent hover:text-foreground data-[active=true]:border-pulse data-[active=true]:bg-pulse/10 data-[active=true]:text-pulse data-[active=true]:font-medium";

function NavItem({ href, label, Icon, active, external, collapsed, onClick }: NavItemProps) {
  if (external) {
    return (
      <a
        href={href}
        target="_blank"
        rel="noopener noreferrer"
        title={collapsed ? label : undefined}
        aria-label={collapsed ? label : undefined}
        className={cn(NAV_ITEM_CLASSES, collapsed && "justify-center px-0")}
        onClick={onClick}
      >
        <Icon size={15} />
        {collapsed ? null : label}
      </a>
    );
  }
  return (
    <Link
      href={href}
      data-active={active ? "true" : undefined}
      title={collapsed ? label : undefined}
      aria-label={collapsed ? label : undefined}
      className={cn(NAV_ITEM_CLASSES, collapsed && "justify-center px-0")}
      onClick={onClick}
    >
      <Icon size={15} />
      {collapsed ? null : label}
    </Link>
  );
}
