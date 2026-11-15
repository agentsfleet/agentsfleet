import { CpuIcon, ChartBarIcon, UsersIcon, ShieldIcon } from "@phosphor-icons/react/ssr";
import type { NavEntry } from "./sidebar-navigation-data";

export const ACCOUNT_NAV: NavEntry[] = [
  { label: "Team", path: "/settings/members", icon: UsersIcon },
  { label: "Security", path: "/settings/security", icon: ShieldIcon },
  { label: "Usage", path: "/settings/billing", icon: ChartBarIcon },
  { label: "Models", path: "settings/models", icon: CpuIcon, workspaceScoped: true },
];
