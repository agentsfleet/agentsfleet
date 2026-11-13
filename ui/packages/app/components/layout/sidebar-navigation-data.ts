import type { Icon as PhosphorIcon } from "@phosphor-icons/react";
import { StackIcon, NotePencilIcon, PuzzlePieceIcon, KeyIcon, PlugsConnectedIcon } from "@phosphor-icons/react/ssr";
import { workspacePath } from "@/lib/workspace-routes";

export type NavEntry = {
  label: string;
  path: string;
  icon: PhosphorIcon;
  workspaceScoped?: boolean;
  external?: boolean;
  analyticsSource?: string;
};


export const NAV_SURFACE = "app_sidebar";
export const NAV_ICON_SIZE = 16;

export const PRIMARY_NAV: NavEntry[] = [
  { label: "New fleet", path: "fleets/new", icon: NotePencilIcon, workspaceScoped: true },
  { label: "Your fleets", path: "fleets", icon: StackIcon, workspaceScoped: true },
  { label: "Plugins", path: "library", icon: PuzzlePieceIcon, workspaceScoped: true },
  { label: "Secrets", path: "secrets", icon: KeyIcon, workspaceScoped: true },
  { label: "Connected apps", path: "integrations", icon: PlugsConnectedIcon, workspaceScoped: true },
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
  if (entry.analyticsSource) return entry.analyticsSource;
  if (entry.external) return `${NAV_SURFACE}_${entry.label.toLowerCase()}`;
  const canonical = entry.workspaceScoped ? `/${entry.path}` : entry.path;
  return `${NAV_SURFACE}_${canonical.replaceAll("/", "_").replace(/^_+/, "")}`;
}
