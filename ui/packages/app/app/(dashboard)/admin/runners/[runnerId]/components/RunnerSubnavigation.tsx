import Link from "next/link";
import type { ComponentType } from "react";
import { ActivityIcon, LayoutListIcon } from "lucide-react";
import { TabNav, type TabNavItem } from "@agentsfleet/design-system";
import { runnerPath, RUNNER_VIEW, type RunnerView } from "@/lib/runner-routes";
import { RAIL_ACTIVITY_LABEL, RAIL_LABEL, RAIL_LEASES_LABEL } from "./runner-copy";

// The two-item strip mirroring FleetSubnavigation's geometry, in the app's one
// tab style: the runner's main object is the lease, so Leases leads and is the
// default landing view.

const ICON_SIZE = 15;

const RUNNER_NAV_ITEMS: { view: RunnerView; label: string; icon: ComponentType<{ size?: number }> }[] = [
  { view: RUNNER_VIEW.leases, label: RAIL_LEASES_LABEL, icon: LayoutListIcon },
  { view: RUNNER_VIEW.activity, label: RAIL_ACTIVITY_LABEL, icon: ActivityIcon },
];

export function RunnerSubnavigation({
  runnerId,
  activeView,
}: {
  runnerId: string;
  activeView: RunnerView;
}) {
  const items: TabNavItem[] = RUNNER_NAV_ITEMS.map(({ view, label, icon: Icon }) => ({
    label,
    href: runnerPath(runnerId, view),
    icon: <Icon size={ICON_SIZE} aria-hidden="true" />,
  }));
  return (
    <TabNav
      label={RAIL_LABEL}
      items={items}
      activeHref={runnerPath(runnerId, activeView)}
      linkComponent={Link}
    />
  );
}
