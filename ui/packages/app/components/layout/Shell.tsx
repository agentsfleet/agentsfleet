"use client";

import { useState } from "react";
import { usePathname } from "next/navigation";
import Link from "next/link";
import {
  LayoutDashboardIcon,
  ActivityIcon,
  SettingsIcon,
  BookOpenIcon,
  SkullIcon,
  KeyRoundIcon,
  CheckCircle2Icon,
  CpuIcon,
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
  WakePulse,
} from "@usezombie/design-system";
import { AuthUserButton } from "@/lib/auth/client";
import { trackNavigationClicked } from "@/lib/analytics/posthog";
import { setActiveWorkspace } from "@/app/(dashboard)/actions";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";
import type { TenantWorkspace } from "@/lib/api/workspaces";
import WorkspaceSwitcher from "./WorkspaceSwitcher";
import ThemeToggle from "./ThemeToggle";

type NavEntry = {
  label: string;
  href: string;
  icon: React.ComponentType<{ size?: number }>;
  external?: boolean;
};

const NAV_SURFACE = "app_sidebar";

// Dashboard sits above the labelled groups as a headerless overview entry.
const TOP_NAV: NavEntry[] = [
  { label: "Dashboard", href: "/", icon: LayoutDashboardIcon },
];

// The live work — what the agents do.
const OPERATIONS_NAV: NavEntry[] = [
  { label: "Agents", href: "/zombies", icon: SkullIcon },
  { label: "Approvals", href: "/approvals", icon: CheckCircle2Icon },
  { label: "Events", href: "/events", icon: ActivityIcon },
];

// What the agents are wired to — secrets, the model brain, the execution fleet.
const CONFIGURATION_NAV: NavEntry[] = [
  { label: "Credentials", href: "/credentials", icon: KeyRoundIcon },
  { label: "Model", href: "/settings/models", icon: CpuIcon },
];

// Platform-admin-only — appended to the Configuration group only when the
// session carries the platform_admin claim (the backend independently gates
// the routes, so this is discoverability, not the security boundary).
const PLATFORM_NAV: NavEntry[] = [
  { label: "Runners", href: "/admin/runners", icon: ServerIcon },
];

const ORGANIZATION_NAV: NavEntry[] = [
  { label: "Settings", href: "/settings", icon: SettingsIcon },
  { label: "Billing", href: "/settings/billing", icon: CreditCardIcon },
];

const BOTTOM_NAV: NavEntry[] = [
  { label: "Docs", href: "https://docs.usezombie.com", icon: BookOpenIcon, external: true },
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
  isPlatformAdmin?: boolean;
};

export default function Shell({
  children,
  workspaces = [],
  activeWorkspaceId = null,
  isPlatformAdmin = false,
}: ShellProps) {
  const pathname = usePathname();
  const activeHref = resolveActiveHref(pathname);
  const isActive = (href: string) => href === activeHref;

  return (
    <div className="grid min-h-screen md:grid-cols-[240px_1fr] grid-rows-[56px_1fr]">
      <header className="col-span-full sticky top-0 z-40 flex items-center gap-4 px-4 md:px-6 border-b border-border bg-background/85 backdrop-blur">
        <MobileNav isActive={isActive} isPlatformAdmin={isPlatformAdmin} />

        <Link
          href="/"
          className="inline-flex items-center gap-2 font-mono text-sm font-medium tracking-tight text-foreground no-underline"
          aria-label="usezombie home"
        >
          <WakePulse
            live
            className="inline-block w-3 h-3 rounded-full bg-pulse"
            aria-hidden="true"
          />
          <span>usezombie</span>
        </Link>

        <div className="flex-1" />

        <WorkspaceSwitcher
          workspaces={workspaces}
          activeId={activeWorkspaceId}
          onSwitch={setActiveWorkspace}
        />

        <ThemeToggle />

        <AuthUserButton appearance={AUTH_APPEARANCE} />
      </header>

      <aside className="hidden md:flex flex-col bg-muted border-r border-border sticky top-14 h-[calc(100vh-56px)] overflow-y-auto py-4">
        <SidebarNav isActive={isActive} onNavigate={() => {}} isPlatformAdmin={isPlatformAdmin} />
      </aside>

      <main className="p-6 md:p-8 overflow-auto">
        <div className="mx-auto w-full max-w-content">{children}</div>
      </main>
    </div>
  );
}

function MobileNav({
  isActive,
  isPlatformAdmin,
}: {
  isActive: (href: string) => boolean;
  isPlatformAdmin: boolean;
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
        <SidebarNav isActive={isActive} onNavigate={() => setOpen(false)} isPlatformAdmin={isPlatformAdmin} />
      </DialogContent>
    </Dialog>
  );
}

type NavProps = {
  isActive: (href: string) => boolean;
  onNavigate: () => void;
  isPlatformAdmin: boolean;
};

function SidebarNav({ isActive, onNavigate, isPlatformAdmin }: NavProps) {
  const configItems = isPlatformAdmin
    ? [...CONFIGURATION_NAV, ...PLATFORM_NAV]
    : CONFIGURATION_NAV;
  return (
    <div className="flex flex-col h-full">
      <NavSection items={TOP_NAV} isActive={isActive} onNavigate={onNavigate} />
      <NavSection label="Operations" items={OPERATIONS_NAV} isActive={isActive} onNavigate={onNavigate} />
      <NavSection label="Configuration" items={configItems} isActive={isActive} onNavigate={onNavigate} />
      <NavSection label="Organization" items={ORGANIZATION_NAV} isActive={isActive} onNavigate={onNavigate} />
      <div className="mt-auto">
        <NavSection items={BOTTOM_NAV} isActive={isActive} onNavigate={onNavigate} />
      </div>
    </div>
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
        <div className="font-mono text-label uppercase tracking-label text-muted-foreground px-2 mb-2">
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

const NAV_ITEM_CLASSES =
  "flex items-center gap-2.5 px-3 py-2 rounded-md font-mono text-eyebrow text-muted-foreground no-underline transition-colors duration-snap ease-snap hover:bg-accent hover:text-foreground data-[active=true]:bg-accent data-[active=true]:text-foreground";

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
