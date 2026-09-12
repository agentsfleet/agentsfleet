import Link from "next/link";
import type { ComponentType } from "react";
import {
  ActivityIcon,
  BrainIcon,
  Code2Icon,
  MessageSquareIcon,
  ZapIcon,
} from "lucide-react";
import { TabNav, type TabNavItem } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";

// The fleet's sections, as the app's one tab style: an underline over a
// hairline rail, the same visual Billing and the settings tabs use. They are
// destinations, not panels, so each is a real link with its own address —
// which is what TabNav is for.

export const FLEET_VIEW = {
  chat: "chat",
  events: "events",
  memory: "memory",
  skill: "skill",
  trigger: "trigger",
} as const;

export type FleetView = (typeof FLEET_VIEW)[keyof typeof FLEET_VIEW];

const ICON_SIZE = 15;

const FLEET_NAV_ITEMS: { view: FleetView; label: string; icon: ComponentType<{ size?: number }> }[] = [
  { view: FLEET_VIEW.chat, label: "Chat", icon: MessageSquareIcon },
  { view: FLEET_VIEW.events, label: "Events", icon: ActivityIcon },
  { view: FLEET_VIEW.memory, label: "Memory", icon: BrainIcon },
  { view: FLEET_VIEW.skill, label: "Skill", icon: Code2Icon },
  { view: FLEET_VIEW.trigger, label: "Trigger", icon: ZapIcon },
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
  const items: TabNavItem[] = FLEET_NAV_ITEMS.map(({ view, label, icon: Icon }) => ({
    label,
    href: hrefFor(baseHref, view),
    icon: <Icon size={ICON_SIZE} aria-hidden="true" />,
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
