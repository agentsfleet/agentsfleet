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

const NAV = [
  { label: "Dashboard", href: "/", icon: LayoutDashboardIcon },
  { label: "Zombies", href: "/zombies", icon: SkullIcon },
  { label: "Credentials", href: "/credentials", icon: KeyRoundIcon },
  { label: "Approvals", href: "/approvals", icon: CheckCircle2Icon },
  { label: "Events", href: "/events", icon: ActivityIcon },
];

const BOTTOM_NAV = [
  { label: "Docs", href: "https://docs.usezombie.com", icon: BookOpenIcon, external: true },
  { label: "Settings", href: "/settings", icon: SettingsIcon },
];

type ShellProps = {
  children: React.ReactNode;
  workspaces?: TenantWorkspace[];
  activeWorkspaceId?: string | null;
};

export default function Shell({
  children,
  workspaces = [],
  activeWorkspaceId = null,
}: ShellProps) {
  const pathname = usePathname();

  const isActive = (href: string) =>
    href === "/" ? pathname === "/" : pathname.startsWith(href);

  return (
    <div className="grid min-h-screen md:grid-cols-[240px_1fr] grid-rows-[56px_1fr]">
      <header className="col-span-full sticky top-0 z-40 flex items-center gap-4 px-4 md:px-6 border-b border-border bg-background/85 backdrop-blur">
        <MobileNav isActive={isActive} />

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

        <nav className="hidden md:flex items-center gap-4">
          <a
            href="https://docs.usezombie.com"
            target="_blank"
            rel="noopener noreferrer"
            className="font-mono text-[12px] text-muted-foreground transition-colors duration-[50ms] hover:text-foreground no-underline"
            onClick={() =>
              trackNavigationClicked({
                source: "app_header_docs",
                surface: "app_header",
                target: "docs",
              })
            }
          >
            docs
          </a>
          <a
            href="https://usezombie.com"
            target="_blank"
            rel="noopener noreferrer"
            className="font-mono text-[12px] text-muted-foreground transition-colors duration-[50ms] hover:text-foreground no-underline"
            onClick={() =>
              trackNavigationClicked({
                source: "app_header_marketing",
                surface: "app_header",
                target: "marketing_site",
              })
            }
          >
            usezombie.com
          </a>
        </nav>

        <AuthUserButton appearance={AUTH_APPEARANCE} />
      </header>

      <aside className="hidden md:flex flex-col bg-muted border-r border-border sticky top-14 h-[calc(100vh-56px)] overflow-y-auto py-4">
        <SidebarNav isActive={isActive} onNavigate={() => {}} />
      </aside>

      <main className="p-6 md:p-8 overflow-auto">{children}</main>
    </div>
  );
}

function MobileNav({ isActive }: { isActive: (href: string) => boolean }) {
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
        <SidebarNav isActive={isActive} onNavigate={() => setOpen(false)} />
      </DialogContent>
    </Dialog>
  );
}

type NavProps = {
  isActive: (href: string) => boolean;
  onNavigate: () => void;
};

function SidebarNav({ isActive, onNavigate }: NavProps) {
  return (
    <div className="flex flex-col h-full">
      <NavGroup label="Operations">
        {NAV.map(({ label, href, icon: Icon }) => (
          <NavItem
            key={href}
            href={href}
            label={label}
            Icon={Icon}
            active={isActive(href)}
            onClick={() => {
              onNavigate();
              trackNavigationClicked({
                source: `app_sidebar_${href === "/" ? "root" : href.replaceAll("/", "_").replace(/^_+/, "")}`,
                surface: "app_sidebar",
                target: href,
              });
            }}
          />
        ))}
      </NavGroup>

      <div className="mt-auto">
        <NavGroup label="More">
          {BOTTOM_NAV.map(({ label, href, icon: Icon, external }) => (
            <NavItem
              key={href}
              href={href}
              label={label}
              Icon={Icon}
              external={external}
              active={external ? false : isActive(href)}
              onClick={() => {
                onNavigate();
                trackNavigationClicked({
                  source: `app_sidebar_more_${label.toLowerCase().replace(/\s+/g, "_")}`,
                  surface: "app_sidebar_more",
                  target: href,
                });
              }}
            />
          ))}
        </NavGroup>
      </div>
    </div>
  );
}

function NavGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="px-3 mb-6">
      <div className="font-mono text-[0.68rem] uppercase tracking-[0.1em] text-muted-foreground px-2 mb-2">
        {label}
      </div>
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
  "flex items-center gap-2.5 px-3 py-2 rounded-md font-mono text-[12px] text-muted-foreground no-underline transition-colors duration-[50ms] hover:bg-accent hover:text-foreground data-[active=true]:bg-accent data-[active=true]:text-foreground";

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
