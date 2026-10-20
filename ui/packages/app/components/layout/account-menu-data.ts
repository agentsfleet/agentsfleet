import { ActivityIcon, BookOpenIcon, CpuIcon, CircleCheckIcon, ChartNoAxesCombinedIcon, KeyRoundIcon, LockKeyholeIcon, LibraryBigIcon, PlugIcon, ServerIcon, UsersIcon, UserRoundIcon } from "lucide-react";
import { SCOPE } from "@/lib/auth/scopes";
import type { NavEntry, PlatformNavEntry } from "./sidebar-navigation-data";

export const WORKSPACE_NAV: NavEntry[] = [
  { label: "Activity", path: "events", icon: ActivityIcon, workspaceScoped: true },
  { label: "Approvals", path: "approvals", icon: CircleCheckIcon, workspaceScoped: true },
  { label: "Models", path: "settings/models", icon: CpuIcon, workspaceScoped: true },
  { label: "Fleet library", path: "library", icon: LibraryBigIcon, workspaceScoped: true },
];

export const PLATFORM_NAV: PlatformNavEntry[] = [
  { label: "Runners", path: "/admin/runners", icon: ServerIcon, scope: SCOPE.RUNNER_READ },
  { label: "Model library", path: "/admin/models", icon: CpuIcon, scope: SCOPE.MODEL_READ },
  { label: "Fleet library", path: "/admin/fleet-libraries", icon: LibraryBigIcon, scope: SCOPE.PLATFORM_LIBRARY_WRITE },
];

// Each person manages the account signup created for them.
export const ACCOUNT_NAV: NavEntry[] = [
  { label: "Profile", path: "/settings/account", icon: UserRoundIcon },
  { label: "Team", path: "/settings/members", icon: UsersIcon },
];

export const ACCOUNT_SHORTCUTS: NavEntry[] = [
  { label: "Usage", path: "/settings/billing", icon: ChartNoAxesCombinedIcon },
];

export const SERVICE_SHORTCUTS: NavEntry[] = [
  { label: "Connect apps", path: "integrations", icon: PlugIcon, workspaceScoped: true },
  { label: "Get API keys", path: "/settings/api-keys", icon: KeyRoundIcon },
  { label: "Manage secrets", path: "secrets", icon: LockKeyholeIcon, workspaceScoped: true },
];

export const BOTTOM_NAV: NavEntry[] = [
  { label: "Learn more", path: "https://docs.agentsfleet.net", icon: BookOpenIcon, external: true, analyticsSource: "app_sidebar_docs" },
];
