import Link from "next/link";
import { TabNav, type TabNavItem } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";

// The fleet's sections, as the app's one tab style: an underline over a
// hairline rail, the same visual Billing and the settings tabs use. They are
// destinations, not panels, so each is a real link with its own address —
// which is what TabNav is for.
//
// Labels only. The glyphs were a rail affordance, where icons line up in a
// column and carry the eye down; in a horizontal strip they are decoration,
// and Billing's three tabs read fine without them.

export const FLEET_VIEW = {
  chat: "chat",
  events: "events",
  memory: "memory",
  skill: "skill",
  trigger: "trigger",
} as const;

export type FleetView = (typeof FLEET_VIEW)[keyof typeof FLEET_VIEW];

const FLEET_NAV_ITEMS: { view: FleetView; label: string }[] = [
  { view: FLEET_VIEW.chat, label: "Chat" },
  { view: FLEET_VIEW.events, label: "Events" },
  { view: FLEET_VIEW.memory, label: "Memory" },
  { view: FLEET_VIEW.skill, label: "Skill" },
  { view: FLEET_VIEW.trigger, label: "Trigger" },
];

const NAV_LABEL = "Fleet sections";

export function resolveFleetView(value: string | undefined): FleetView | null {
  switch (value) {
    case undefined:
    case FLEET_VIEW.chat:
      return FLEET_VIEW.chat;
    case FLEET_VIEW.events:
    case FLEET_VIEW.memory:
    case FLEET_VIEW.skill:
    case FLEET_VIEW.trigger:
      return value;
    default:
      return null;
  }
}

/** Chat is the fleet's own address; every other view rides a query. */
function hrefFor(baseHref: string, view: FleetView): string {
  return view === FLEET_VIEW.chat ? baseHref : `${baseHref}?view=${view}`;
}

export function FleetSubnavigation({
  workspaceId,
  fleetId,
  activeView,
}: {
  workspaceId: string;
  fleetId: string;
  activeView: FleetView;
}) {
  const baseHref = workspacePath(workspaceId, `fleets/${fleetId}`);
  const items: TabNavItem[] = FLEET_NAV_ITEMS.map(({ view, label }) => ({
    label,
    href: hrefFor(baseHref, view),
  }));
  return (
    <TabNav
      label={NAV_LABEL}
      items={items}
      activeHref={hrefFor(baseHref, activeView)}
      linkComponent={Link}
    />
  );
}
