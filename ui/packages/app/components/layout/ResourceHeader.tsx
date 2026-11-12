"use client";

import { usePathname } from "next/navigation";
import { CpuIcon, UsersIcon, ChartBarIcon, ShieldCheckIcon, UserCircleIcon, KeyIcon } from "@phosphor-icons/react/ssr";
import { PageHeader } from "@agentsfleet/design-system";
import { PRIMARY_NAV } from "./sidebar-navigation-data";

const ACCOUNT_PAGES = [
  {path: "settings/models", label: "Models", icon: CpuIcon},
  {path: "settings/members", label: "Team", icon: UsersIcon},
  {path: "settings/billing", label: "Usage", icon: ChartBarIcon},
  {path: "settings/security", label: "Security", icon: ShieldCheckIcon},
  {path: "settings/account", label: "Profile", icon: UserCircleIcon},
  {path: "settings/api-keys", label: "API keys", icon: KeyIcon},
] as const;

/** Display only: routing, authorization and account selection keep their owners. */
export function ResourceHeader() {
  const pathname = usePathname();
  const page = [...PRIMARY_NAV, ...ACCOUNT_PAGES].find(entry => entry.path !== "fleets/new" && pathname.endsWith(`/${entry.path}`));
  if (!page) return null;
  const Icon = page.icon;
  return <PageHeader data-resource-header className="-mx-[var(--app-canvas-gutter)] -mt-[var(--app-canvas-block)] mb-2xl h-16 shrink-0 flex-row items-center gap-lg px-xl pb-0 sm:mb-4xl sm:px-2xl">
    <span className="flex items-center gap-lg text-label font-medium"><Icon size={16} className="hidden sm:block" aria-hidden="true" />{page.label}</span>
  </PageHeader>;
}
