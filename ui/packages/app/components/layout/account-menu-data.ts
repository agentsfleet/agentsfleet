import { CpuIcon, ChartLineUpIcon, UsersIcon, UserCircleIcon, ShieldCheckIcon } from "@phosphor-icons/react/ssr";
import type { NavEntry } from "./sidebar-navigation-data";

export const ACCOUNT_NAV: NavEntry[] = [
  { label: "Personal account", path: "/settings/account/sign-in", icon: UserCircleIcon },
  { label: "Team", path: "/settings/members", icon: UsersIcon },
  { label: "Security", path: "/settings/security", icon: ShieldCheckIcon },
  { label: "Usage", path: "/settings/billing", icon: ChartLineUpIcon },
  { label: "Models", path: "settings/models", icon: CpuIcon, workspaceScoped: true },
];
