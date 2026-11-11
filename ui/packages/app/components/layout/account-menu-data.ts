import { PulseIcon, BookOpenIcon, CpuIcon, CheckCircleIcon, ChartLineUpIcon, KeyIcon, LockKeyIcon, PlugsConnectedIcon, UsersIcon, UserCircleIcon, ShieldCheckIcon } from "@phosphor-icons/react/ssr";
import type { NavEntry } from "./sidebar-navigation-data";

export const WORKSPACE_NAV: NavEntry[] = [
  { label: "Activity", path: "events", icon: PulseIcon, workspaceScoped: true },
  { label: "Approvals", path: "approvals", icon: CheckCircleIcon, workspaceScoped: true },
  { label: "Models", path: "settings/models", icon: CpuIcon, workspaceScoped: true },
];


// Each person manages the account signup created for them.
export const ACCOUNT_NAV: NavEntry[] = [
  { label: "Profile", path: "/settings/account", icon: UserCircleIcon },
  { label: "Security", path: "/settings/security", icon: ShieldCheckIcon },
  { label: "Team", path: "/settings/members", icon: UsersIcon },
];

export const ACCOUNT_SHORTCUTS: NavEntry[] = [
  { label: "Usage", path: "/settings/billing", icon: ChartLineUpIcon },
];

export const SERVICE_SHORTCUTS: NavEntry[] = [
  { label: "Connect apps", path: "integrations", icon: PlugsConnectedIcon, workspaceScoped: true },
  { label: "Get API keys", path: "/settings/api-keys", icon: KeyIcon },
  { label: "Manage secrets", path: "secrets", icon: LockKeyIcon, workspaceScoped: true },
];

export const BOTTOM_NAV: NavEntry[] = [
  { label: "Learn more", path: "https://docs.agentsfleet.net", icon: BookOpenIcon, external: true, analyticsSource: "app_sidebar_docs" },
];
