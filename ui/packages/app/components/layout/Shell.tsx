"use client";

import { useEffect, useState } from "react";
import { usePathname } from "next/navigation";
import Link from "next/link";
import {
  ActivityIcon,
  BookOpenIcon,
  BotIcon,
  CheckCircle2Icon,
  CpuIcon,
  CoinsIcon,
  KeyIcon,
  KeyRoundIcon,
  LibraryIcon,
  LinkIcon,
  CreditCardIcon,
  ServerIcon,
  MenuIcon,
  PanelLeftIcon,
} from "lucide-react";
import {
  Button,
  cn,
  Dialog,
  DialogContent,
  DialogTitle,
  DialogTrigger,
  EYEBROW_CLASS,
  Nav,
  WakePulse,
} from "@agentsfleet/design-system";
import { setAnalyticsContext, trackNavigationClicked } from "@/lib/analytics/posthog";
import { SCOPE } from "@/lib/auth/scopes";
import { workspaceIdFromPath, workspacePath } from "@/lib/workspace-routes";
import GettingStartedWidget from "@/components/layout/GettingStartedWidget";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import WorkspaceSwitcher from "./WorkspaceSwitcher";
import ThemeToggle from "./ThemeToggle";
import ClientOnlyAuthUserButton from "./ClientOnlyAuthUserButton";

type NavEntry = {
  label: string;
  // Workspace-scoped items store their sub-path under `/w/<id>/` ("" = the
  // workspace home, "fleets", "settings/models"); tenant/platform items store
  // an absolute root path ("/settings/api-keys"); external items store a URL.
  path: string;
  icon: React.ComponentType<{ size?: number }>;
  workspaceScoped?: boolean;
  // Home matches its resolved path exactly (else it'd claim every deeper route).
  exact?: boolean;
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

// The Wall (Fleets) is the workspace's only entry point — there is no dashboard
// route and no dashboard nav entry (a nav item whose only job is to redirect is
// a dead link, M132 single-route refactor). Fleets leads the nav.

// The live work — what the fleets do.
const OPERATIONS_NAV: NavEntry[] = [
  { label: "Fleets", path: "fleets", icon: BotIcon, workspaceScoped: true },
  { label: "Approvals", path: "approvals", icon: CheckCircle2Icon, workspaceScoped: true },
  { label: "Events", path: "events", icon: ActivityIcon, workspaceScoped: true },
];

// What the fleets are wired to — the model brain (which now also hosts the
// write-only secret vault) and the tool connectors, each its own destination;
// plus the execution fleet for platform admins.
const CONFIGURATION_NAV: NavEntry[] = [
  { label: "Models", path: "settings/models", icon: CpuIcon, workspaceScoped: true },
  { label: "Integrations", path: "integrations", icon: LinkIcon, workspaceScoped: true },
  { label: "Secrets", path: "secrets", icon: KeyRoundIcon, workspaceScoped: true },
];

// Platform-operator surfaces — each appended to the Configuration group only
// when the session token carries that surface's read scope (the backend
// independently gates the routes, so this is discoverability, not the security
// boundary). Platform surfaces are tenant-wide: they stay at the root path,
// carrying no workspace segment.
const PLATFORM_NAV: PlatformNavEntry[] = [
  { label: "Runners", path: "/admin/runners", icon: ServerIcon, scope: SCOPE.RUNNER_READ },
  { label: "Model library", path: "/admin/models", icon: CoinsIcon, scope: SCOPE.MODEL_READ },
  // Gated on the write scope because the platform catalog has no read route —
  // `platform-library:write` is the only rung the backend defines for it.
  {
    label: "Fleet library",
    path: "/admin/fleet-libraries",
    icon: LibraryIcon,
    scope: SCOPE.PLATFORM_LIBRARY_WRITE,
  },
];

// Tenant-scoped surfaces — one billing/key surface per tenant, so they too stay
// at the root path with no workspace segment.
const ORGANIZATION_NAV: NavEntry[] = [
  { label: "API Keys", path: "/settings/api-keys", icon: KeyIcon },
  { label: "Billing", path: "/settings/billing", icon: CreditCardIcon },
];

const BOTTOM_NAV: NavEntry[] = [
  { label: "Docs", path: "https://docs.agentsfleet.net", icon: BookOpenIcon, external: true },
];

const INTERNAL_NAV: NavEntry[] = [
  ...OPERATIONS_NAV,
  ...CONFIGURATION_NAV,
  ...PLATFORM_NAV,
  ...ORGANIZATION_NAV,
];

// Resolves an entry to its concrete href. Workspace-scoped items are prefixed
// with `/w/<id>` when a workspace is in scope; with no workspace at all they
// fall back to `/` (the entry redirect). Tenant/platform/external items are
// their path verbatim.
function resolveHref(entry: NavEntry, workspaceId: string | null): string {
  if (entry.external || !entry.workspaceScoped) return entry.path;
  return workspaceId ? workspacePath(workspaceId, entry.path) : "/";
}

// The stable analytics slug for an entry — derived from its canonical absolute
// path so the workspace id never leaks into the event name and the slug is
// unchanged from the pre-URL nav (root / fleets / settings_models / …).
function navSource(entry: NavEntry): string {
  if (entry.external) return `${NAV_SURFACE}_${entry.label.toLowerCase()}`;
  const canonical = entry.workspaceScoped
    ? (entry.path === "" ? "/" : `/${entry.path}`)
    : entry.path;
  return `${NAV_SURFACE}_${canonical === "/" ? "root" : canonical.replaceAll("/", "_").replace(/^_+/, "")}`;
}

function resolveActiveHref(
  entries: { href: string; exact?: boolean }[],
  pathname: string,
): string {
  let active = "";
  for (const { href, exact } of entries) {
    const hit = exact
      ? pathname === href
      : pathname === href || pathname.startsWith(`${href}/`);
    if (hit && href.length > active.length) active = href;
  }
  return active;
}

type ShellProps = {
  children: React.ReactNode;
  workspaces?: TenantWorkspace[];
  /** Operator scopes on the session token; gate the platform nav per-surface. */
  operatorScopes?: string[];
};

export default function Shell({
  children,
  workspaces = [],
  operatorScopes = [],
}: ShellProps) {
  const pathname = usePathname();
  // The workspace in view comes from the route (`/w/<id>/…`); `null` on
  // tenant/platform pages. The link target for workspace nav items falls back to
  // the first owned workspace so those links still resolve from a tenant page.
  const activeWorkspaceId = workspaceIdFromPath(pathname);
  const linkWorkspaceId = activeWorkspaceId ?? workspaces[0]?.id ?? null;

  const activeHref = resolveActiveHref(
    INTERNAL_NAV.map((entry) => ({ href: resolveHref(entry, linkWorkspaceId), exact: entry.exact })),
    pathname,
  );
  // `/` is only ever a resolved href for workspace items when the tenant owns no
  // workspace (the entry-redirect stub) — it must never light up a nav item, or
  // the whole sidebar reads active on the empty state.
  const isActive = (href: string) => href !== "" && href !== "/" && href === activeHref;

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
        <MobileNav isActive={isActive} workspaceId={linkWorkspaceId} operatorScopes={operatorScopes} />

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
          href={linkWorkspaceId ? workspacePath(linkWorkspaceId) : "/"}
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

        <WorkspaceSwitcher workspaces={workspaces} activeId={linkWorkspaceId} />

        <ThemeToggle />

        <ClientOnlyAuthUserButton />
      </header>

      <aside
        id={SIDEBAR_NAV_ID}
        className="hidden md:flex flex-col bg-muted border-r border-border sticky top-14 h-[calc(100vh-56px)] overflow-y-auto py-4"
      >
        <SidebarNav
          isActive={isActive}
          workspaceId={linkWorkspaceId}
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
  workspaceId,
  operatorScopes,
}: {
  isActive: (href: string) => boolean;
  workspaceId: string | null;
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
        <SidebarNav isActive={isActive} workspaceId={workspaceId} onNavigate={() => setOpen(false)} operatorScopes={operatorScopes} collapsed={false} />
      </DialogContent>
    </Dialog>
  );
}

type NavProps = {
  isActive: (href: string) => boolean;
  workspaceId: string | null;
  onNavigate: () => void;
  operatorScopes: string[];
  collapsed: boolean;
};

function SidebarNav({ isActive, workspaceId, onNavigate, operatorScopes, collapsed }: NavProps) {
  // Each platform surface appears iff the session token holds its read scope;
  // a token with neither scope sees the plain Configuration group.
  const platformItems = PLATFORM_NAV.filter((entry) => operatorScopes.includes(entry.scope));
  const configItems = [...CONFIGURATION_NAV, ...platformItems];
  return (
    <Nav aria-label="Primary" className="flex flex-col h-full">
      <NavSection label="Automations" items={OPERATIONS_NAV} isActive={isActive} workspaceId={workspaceId} onNavigate={onNavigate} collapsed={collapsed} />
      <NavSection label="Configuration" items={configItems} isActive={isActive} workspaceId={workspaceId} onNavigate={onNavigate} collapsed={collapsed} />
      <NavSection label="Organization" items={ORGANIZATION_NAV} isActive={isActive} workspaceId={workspaceId} onNavigate={onNavigate} collapsed={collapsed} />
      <div className="mt-auto">
        {/* The onboarding checklist's only home once a fleet exists — pinned
            above Docs, hidden when there is no workspace or once dismissed. The
            expanded rail is suppressed while the nav is collapsed to 64px. */}
        {workspaceId && !collapsed ? <GettingStartedWidget workspaceId={workspaceId} /> : null}
        <NavSection items={BOTTOM_NAV} isActive={isActive} workspaceId={workspaceId} onNavigate={onNavigate} collapsed={collapsed} />
      </div>
    </Nav>
  );
}

function NavSection({
  label,
  items,
  isActive,
  workspaceId,
  onNavigate,
  collapsed,
}: {
  label?: string;
  items: NavEntry[];
  isActive: (href: string) => boolean;
  workspaceId: string | null;
  onNavigate: () => void;
  collapsed: boolean;
}) {
  return (
    <NavGroup label={label} collapsed={collapsed}>
      {items.map((entry) => {
        const href = resolveHref(entry, workspaceId);
        return (
          <NavItem
            key={entry.label}
            href={href}
            label={entry.label}
            Icon={entry.icon}
            external={entry.external}
            active={entry.external ? false : isActive(href)}
            collapsed={collapsed}
            onClick={() => {
              onNavigate();
              trackNavigationClicked({
                source: navSource(entry),
                surface: NAV_SURFACE,
                target: href,
              });
            }}
          />
        );
      })}
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
