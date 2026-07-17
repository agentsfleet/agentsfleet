"use client";

import type { ComponentType, ReactNode } from "react";
import { useEffect, useId, useState } from "react";
import Link from "next/link";
import {
  ActivityIcon,
  BookOpenIcon,
  BotIcon,
  BoxesIcon,
  BrainCircuitIcon,
  CheckCircle2Icon,
  ChevronDownIcon,
  ChevronRightIcon,
  CreditCardIcon,
  KeyIcon,
  KeyRoundIcon,
  LibraryIcon,
  MenuIcon,
  PlugIcon,
  ServerIcon,
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
} from "@agentsfleet/design-system";
import { trackNavigationClicked } from "@/lib/analytics/posthog";
import { SCOPE } from "@/lib/auth/scopes";
import { workspacePath } from "@/lib/workspace-routes";
import GettingStartedWidget from "./GettingStartedWidget";

type NavEntry = {
  label: string;
  path: string;
  icon: ComponentType<{ size?: number }>;
  workspaceScoped?: boolean;
  external?: boolean;
};

type PlatformNavEntry = NavEntry & { scope: string };

type SidebarNavigationProps = {
  pathname: string;
  workspaceId: string | null;
  operatorScopes: string[];
  collapsed: boolean;
  onNavigate: () => void;
};

const NAV_SURFACE = "app_sidebar";

const OPERATIONS_NAV: NavEntry[] = [
  { label: "Fleets", path: "fleets", icon: BotIcon, workspaceScoped: true },
  { label: "Approvals", path: "approvals", icon: CheckCircle2Icon, workspaceScoped: true },
  { label: "Events", path: "events", icon: ActivityIcon, workspaceScoped: true },
];

const CONFIGURATION_NAV: NavEntry[] = [
  { label: "Models", path: "settings/models", icon: BrainCircuitIcon, workspaceScoped: true },
  { label: "Integrations", path: "integrations", icon: PlugIcon, workspaceScoped: true },
  { label: "Secrets", path: "secrets", icon: KeyRoundIcon, workspaceScoped: true },
];

const PLATFORM_NAV: PlatformNavEntry[] = [
  { label: "Runners", path: "/admin/runners", icon: ServerIcon, scope: SCOPE.RUNNER_READ },
  { label: "Model library", path: "/admin/models", icon: BoxesIcon, scope: SCOPE.MODEL_READ },
  {
    label: "Fleet library",
    path: "/admin/fleet-libraries",
    icon: LibraryIcon,
    scope: SCOPE.PLATFORM_LIBRARY_WRITE,
  },
];

const ORGANIZATION_NAV: NavEntry[] = [
  { label: "API Keys", path: "/settings/api-keys", icon: KeyIcon },
  { label: "Billing", path: "/settings/billing", icon: CreditCardIcon },
];

const BOTTOM_NAV: NavEntry[] = [
  { label: "Docs", path: "https://docs.agentsfleet.net", icon: BookOpenIcon, external: true },
];

function resolveHref(entry: NavEntry, workspaceId: string | null): string {
  if (entry.external || !entry.workspaceScoped) return entry.path;
  return workspaceId ? workspacePath(workspaceId, entry.path) : "/";
}

// The single active winner: the LONGEST resolved href that prefixes the current
// path. Computing it once — rather than letting each item self-decide — keeps
// the invariant "exactly one nav item is active, and a nested route never lights
// a sibling" true by construction. It holds even when a future nav path prefixes
// another (e.g. `settings` alongside `settings/models`), which a per-item check
// would double-light. `/` and `""` are entry-redirect stubs and never win.
export function resolveActiveHref(hrefs: string[], pathname: string): string {
  let active = "";
  for (const href of hrefs) {
    if (href === "" || href === "/") continue;
    const hit = pathname === href || pathname.startsWith(`${href}/`);
    if (hit && href.length > active.length) active = href;
  }
  return active;
}

function navSource(entry: NavEntry): string {
  if (entry.external) return `${NAV_SURFACE}_${entry.label.toLowerCase()}`;
  const canonical = entry.workspaceScoped ? `/${entry.path}` : entry.path;
  return `${NAV_SURFACE}_${canonical.replaceAll("/", "_").replace(/^_+/, "")}`;
}

export function SidebarNavigation({
  pathname,
  workspaceId,
  operatorScopes,
  collapsed,
  onNavigate,
}: SidebarNavigationProps) {
  const platformItems = PLATFORM_NAV.filter((entry) => operatorScopes.includes(entry.scope));
  // Resolve the single active href across every internal item so no two items
  // (and no group) can read active at once.
  const activeHref = resolveActiveHref(
    [...OPERATIONS_NAV, ...CONFIGURATION_NAV, ...platformItems, ...ORGANIZATION_NAV].map((entry) =>
      resolveHref(entry, workspaceId),
    ),
    pathname,
  );
  const shared = { activeHref, workspaceId, onNavigate, collapsed };
  return (
    <Nav aria-label="Primary" className="flex flex-col h-full">
      <NavSection label="Automations" items={OPERATIONS_NAV} {...shared} />
      <NavSection label="Configuration" items={CONFIGURATION_NAV} {...shared} />
      <PlatformSection items={platformItems} {...shared} />
      <NavSection label="Organization" items={ORGANIZATION_NAV} {...shared} />
      <div className="mt-auto">
        {workspaceId && !collapsed ? <GettingStartedWidget workspaceId={workspaceId} /> : null}
        <NavSection items={BOTTOM_NAV} {...shared} />
      </div>
    </Nav>
  );
}

type NavListProps = {
  activeHref: string;
  workspaceId: string | null;
  onNavigate: () => void;
  collapsed: boolean;
};

function PlatformSection({ items, activeHref, workspaceId, onNavigate, collapsed }: NavListProps & { items: NavEntry[] }) {
  const active = items.some((entry) => resolveHref(entry, workspaceId) === activeHref);
  // Open by default: platform operators see Runners / Model library / Fleet
  // library without hunting for them (they used to live inline in Configuration).
  // Still collapsible to declutter, and force-open whenever the current route
  // lives inside the group.
  const [open, setOpen] = useState(true);
  const regionId = useId();

  useEffect(() => {
    if (active) setOpen(true);
  }, [active]);

  if (items.length === 0) return null;
  if (collapsed) return <NavSection items={items} {...{ activeHref, workspaceId, onNavigate, collapsed }} />;
  return (
    <NavGroup>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={regionId}
        className={cn(
          EYEBROW_CLASS,
          "flex w-full items-center justify-between px-2 mb-2 text-muted-foreground hover:text-foreground",
          active && "text-foreground",
        )}
        onClick={() => setOpen((current) => !current)}
      >
        <span>Platform</span>
        {open ? <ChevronDownIcon size={14} /> : <ChevronRightIcon size={14} />}
      </button>
      <div id={regionId} hidden={!open} className="flex flex-col gap-0.5">
        {open ? <NavItems items={items} {...{ activeHref, workspaceId, onNavigate, collapsed }} /> : null}
      </div>
    </NavGroup>
  );
}

export function MobileNavigation(props: Omit<SidebarNavigationProps, "collapsed" | "onNavigate">) {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button type="button" aria-label="Open navigation" variant="ghost" size="icon" className="md:hidden -ml-2">
          <MenuIcon size={18} />
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-xs">
        <DialogTitle className="sr-only">Navigation</DialogTitle>
        <SidebarNavigation {...props} collapsed={false} onNavigate={() => setOpen(false)} />
      </DialogContent>
    </Dialog>
  );
}

function NavSection({ label, items, ...props }: NavListProps & { label?: string; items: NavEntry[] }) {
  return (
    <NavGroup label={label} collapsed={props.collapsed}>
      <NavItems items={items} {...props} />
    </NavGroup>
  );
}

function NavItems({ items, activeHref, workspaceId, onNavigate, collapsed }: NavListProps & { items: NavEntry[] }) {
  return items.map((entry) => {
    const href = resolveHref(entry, workspaceId);
    return (
      <NavItem
        key={entry.label}
        href={href}
        label={entry.label}
        Icon={entry.icon}
        external={entry.external}
        active={!entry.external && href !== "" && href === activeHref}
        collapsed={collapsed}
        onClick={() => {
          onNavigate();
          trackNavigationClicked({ source: navSource(entry), surface: NAV_SURFACE, target: href });
        }}
      />
    );
  });
}

function NavGroup({ label, collapsed, children }: { label?: string; collapsed?: boolean; children: ReactNode }) {
  return (
    <div className="px-3 mb-6">
      {label && !collapsed ? <div className={cn(EYEBROW_CLASS, "text-muted-foreground px-2 mb-2")}>{label}</div> : null}
      {children}
    </div>
  );
}

const NAV_ITEM_CLASSES =
  "flex items-center gap-2.5 px-3 py-2 rounded-r-md border-l-2 border-transparent font-mono text-body-sm text-muted-foreground no-underline transition duration-snap ease-snap motion-safe:hover:translate-x-px hover:bg-accent hover:text-foreground data-[active=true]:border-pulse data-[active=true]:bg-pulse/10 data-[active=true]:text-pulse data-[active=true]:font-medium";

function NavItem({
  href,
  label,
  Icon,
  active,
  external,
  collapsed,
  onClick,
}: {
  href: string;
  label: string;
  Icon: ComponentType<{ size?: number }>;
  active?: boolean;
  external?: boolean;
  collapsed?: boolean;
  onClick: () => void;
}) {
  const content = (
    <>
      <Icon size={15} />
      {collapsed ? null : label}
    </>
  );
  const common = {
    title: collapsed ? label : undefined,
    "aria-label": collapsed ? label : undefined,
    className: cn(NAV_ITEM_CLASSES, collapsed && "justify-center px-0"),
    onClick,
  };
  if (external) {
    return <a href={href} target="_blank" rel="noopener noreferrer" {...common}>{content}</a>;
  }
  return <Link href={href} data-active={active ? "true" : undefined} {...common}>{content}</Link>;
}
