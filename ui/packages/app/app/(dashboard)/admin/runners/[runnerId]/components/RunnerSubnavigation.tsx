import type { ComponentType } from "react";
import Link from "next/link";
import { ActivityIcon, LayoutListIcon } from "lucide-react";
import { Nav, NavItem } from "@agentsfleet/design-system";
import { runnerPath, RUNNER_VIEW, type RunnerView } from "@/lib/runner-routes";
import { RAIL_ACTIVITY_LABEL, RAIL_LABEL, RAIL_LEASES_LABEL } from "./runner-copy";

// The two-item strip mirroring FleetSubnavigation's geometry: the runner's
// main object is the lease, so Leases leads and is the default landing view.

type RunnerNavItem = {
  view: RunnerView;
  label: string;
  icon: ComponentType<{ size?: number }>;
};

const RUNNER_NAV_ITEMS: RunnerNavItem[] = [
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
  return (
    <Nav
      aria-label={RAIL_LABEL}
      className="flex gap-xs overflow-x-auto border-b border-border pb-md"
    >
      {RUNNER_NAV_ITEMS.map((item) => {
        const Icon = item.icon;
        const active = item.view === activeView;
        return (
          <NavItem asChild active={active} key={item.view}>
            <Link href={runnerPath(runnerId, item.view)}>
              <Icon size={15} />
              {item.label}
            </Link>
          </NavItem>
        );
      })}
    </Nav>
  );
}
