import type { LucideIcon } from "lucide-react";
import { ActivityIcon, BookOpenIcon, BotIcon, BoxesIcon, BrainCircuitIcon, CircleCheckIcon, CreditCardIcon, KeyIcon, KeyRoundIcon, LibraryIcon, PlugIcon, PlusIcon, ServerIcon, UsersIcon, UserIcon } from "lucide-react";
import { SCOPE } from "@/lib/auth/scopes";
import { workspacePath } from "@/lib/workspace-routes";

export type NavEntry = {
  label: string;
  path: string;
  icon: LucideIcon;
  workspaceScoped?: boolean;
  external?: boolean;
};

export type PlatformNavEntry = NavEntry & { scope?: string };

export const NAV_SURFACE = "app_sidebar";

export const PRIMARY_NAV: NavEntry[] = [
  { label: "New fleet", path: "fleets/new", icon: PlusIcon, workspaceScoped: true },
  { label: "Your fleets", path: "fleets", icon: BotIcon, workspaceScoped: true },
];

export const WORKSPACE_NAV: NavEntry[] = [
  { label: "Activity", path: "events", icon: ActivityIcon, workspaceScoped: true },
  { label: "Approvals", path: "approvals", icon: CircleCheckIcon, workspaceScoped: true },
  { label: "Models", path: "settings/models", icon: BrainCircuitIcon, workspaceScoped: true },
  { label: "Integrations", path: "integrations", icon: PlugIcon, workspaceScoped: true },
  { label: "Secrets", path: "secrets", icon: KeyRoundIcon, workspaceScoped: true },
  { label: "Fleet library", path: "library", icon: LibraryIcon, workspaceScoped: true },
];

export const PLATFORM_NAV: PlatformNavEntry[] = [
  { label: "Runners", path: "/admin/runners", icon: ServerIcon, scope: SCOPE.RUNNER_READ },
  { label: "Model library", path: "/admin/models", icon: BoxesIcon, scope: SCOPE.MODEL_READ },
  {
    label: "Fleet library",
    path: "/admin/fleet-libraries",
    icon: LibraryIcon,
    scope: SCOPE.PLATFORM_LIBRARY_WRITE,
  },
];

// Every person owns the account signup made them, so Members is always theirs
// to manage; a joined account's people are its owner's page, not this one.
export const ACCOUNT_NAV: NavEntry[] = [
  { label: "Profile", path: "/settings/account", icon: UserIcon },
  { label: "Team", path: "/settings/members", icon: UsersIcon },
  { label: "API Keys", path: "/settings/api-keys", icon: KeyIcon },
  { label: "Billing", path: "/settings/billing", icon: CreditCardIcon },
];

export const BOTTOM_NAV: NavEntry[] = [
  { label: "Docs", path: "https://docs.agentsfleet.net", icon: BookOpenIcon, external: true },
];

export function resolveHref(entry: NavEntry, workspaceId: string | null): string {
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

export function navSource(entry: NavEntry): string {
  if (entry.external) return `${NAV_SURFACE}_${entry.label.toLowerCase()}`;
  const canonical = entry.workspaceScoped ? `/${entry.path}` : entry.path;
  return `${NAV_SURFACE}_${canonical.replaceAll("/", "_").replace(/^_+/, "")}`;
}
