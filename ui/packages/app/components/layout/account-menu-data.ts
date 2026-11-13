import { CpuIcon, ChartLineUpIcon, UsersIcon, KeyIcon } from "@phosphor-icons/react/ssr";
import type { NavEntry } from "./sidebar-navigation-data";

export const ACCOUNT_NAV: NavEntry[] = [
  { label: "Team", path: "/settings/members", icon: UsersIcon },
  { label: "API Keys", path: "/settings/security", icon: KeyIcon },
  { label: "Usage", path: "/settings/billing", icon: ChartLineUpIcon },
  { label: "Models", path: "settings/models", icon: CpuIcon, workspaceScoped: true },
];
