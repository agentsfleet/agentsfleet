"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { PageHeader, PageTitle, TabNav, type TabNavItem } from "@agentsfleet/design-system";
import { trackNavigationClicked } from "@/lib/analytics/posthog";

const NAV_SURFACE = "settings_tabs";

// Org-settings sub-sections. Billing and Models are their own top-level sidebar
// destinations. Defaults + Security are scaffolded (the routes exist) but
// omitted here until built — add them back to surface them.
const SETTINGS_TABS: TabNavItem[] = [
  { label: "Basic Info", href: "/settings" },
  { label: "API Keys", href: "/settings/api-keys" },
];

// "/settings" is the index — match it only exactly so it doesn't light up on
// nested tabs; deeper tabs match themselves and any future children. Masked
// sub-routes (/settings/defaults, /settings/security) have no tab entry, so
// they highlight nothing rather than falsely lighting up Basic Info.
function activeHref(pathname: string): string {
  for (const tab of SETTINGS_TABS) {
    if (tab.href === "/settings") continue;
    if (pathname === tab.href || pathname.startsWith(`${tab.href}/`)) return tab.href;
  }
  return pathname === "/settings" ? "/settings" : "";
}

function tabSource(href: string): string {
  // Last path segment as the slug. `lastIndexOf` always returns a number, so
  // `slice` yields a string — no possibly-undefined `pop()` fallback branch.
  const slug = href === "/settings" ? "basic" : href.slice(href.lastIndexOf("/") + 1);
  return `${NAV_SURFACE}_${slug}`;
}

// Thin Next adapter over the design-system <TabNav>: injects routing
// (usePathname + <Link>), the settings tab set, and nav analytics. The nav
// rendering/styling lives in the design-system primitive.
export default function SettingsTabs() {
  const pathname = usePathname();
  return (
    <div className="space-y-6">
      <PageHeader>
        <PageTitle>Settings</PageTitle>
      </PageHeader>
      <TabNav
        label="Settings sections"
        items={SETTINGS_TABS}
        activeHref={activeHref(pathname)}
        linkComponent={Link}
        onNavigate={(href) =>
          trackNavigationClicked({ source: tabSource(href), surface: NAV_SURFACE, target: href })
        }
      />
    </div>
  );
}
