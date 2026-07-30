import type { ComponentType } from "react";
import Link from "next/link";
import { ActivityIcon, LayoutListIcon } from "lucide-react";
import { Nav } from "@agentsfleet/design-system";
import { runnerPath, RUNNER_VIEW, type RunnerView } from "@/lib/runner-routes";
import { RAIL_ACTIVITY_LABEL, RAIL_LABEL, RAIL_LEASES_LABEL } from "./runner-copy";

// The two-item rail mirroring FleetSubnavigation's geometry: the runner's main
// object is the lease, so Leases leads and is the default landing view.

type RunnerNavItem = {
  view: RunnerView;
  label: string;
  icon: ComponentType<{ size?: number }>;
};

const RUNNER_NAV_ITEMS: RunnerNavItem[] = [
  { view: RUNNER_VIEW.leases, label: RAIL_LEASES_LABEL, icon: LayoutListIcon },
  { view: RUNNER_VIEW.activity, label: RAIL_ACTIVITY_LABEL, icon: ActivityIcon },
];

const RUNNER_NAV_ITEM_CLASS =
  "flex min-h-11 shrink-0 items-center gap-md rounded-md px-md py-sm font-mono text-body-sm text-muted-foreground no-underline transition duration-snap ease-snap hover:bg-accent hover:text-foreground data-[active=true]:bg-accent data-[active=true]:font-medium data-[active=true]:text-foreground";

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
      className="flex gap-xs overflow-x-auto border-b border-border pb-md lg:min-h-full lg:w-56 lg:shrink-0 lg:flex-col lg:overflow-visible lg:border-b-0 lg:border-r lg:pb-0 lg:pr-xl"
    >
      {RUNNER_NAV_ITEMS.map((item) => {
        const Icon = item.icon;
        const active = item.view === activeView;
        return (
          <Link
            key={item.view}
            href={runnerPath(runnerId, item.view)}
            aria-current={active ? "page" : undefined}
            data-active={active ? "true" : undefined}
            className={RUNNER_NAV_ITEM_CLASS}
          >
            <Icon size={15} />
            {item.label}
          </Link>
        );
      })}
    </Nav>
  );
}
