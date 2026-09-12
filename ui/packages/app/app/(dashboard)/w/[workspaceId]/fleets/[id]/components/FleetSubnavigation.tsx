import type { ComponentType } from "react";
import Link from "next/link";
import {
  ActivityIcon,
  BrainIcon,
  Code2Icon,
  MessageSquareIcon,
  ZapIcon,
} from "lucide-react";
import { Nav, NavItem } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";

export const FLEET_VIEW = {
  chat: "chat",
  events: "events",
  memory: "memory",
  skill: "skill",
  trigger: "trigger",
} as const;

export type FleetView = (typeof FLEET_VIEW)[keyof typeof FLEET_VIEW];

type FleetNavItem = {
  view: FleetView;
  label: string;
  icon: ComponentType<{ size?: number }>;
};

const FLEET_NAV_ITEMS: FleetNavItem[] = [
  { view: FLEET_VIEW.chat, label: "Chat", icon: MessageSquareIcon },
  { view: FLEET_VIEW.events, label: "Events", icon: ActivityIcon },
  { view: FLEET_VIEW.memory, label: "Memory", icon: BrainIcon },
  { view: FLEET_VIEW.skill, label: "Skill", icon: Code2Icon },
  { view: FLEET_VIEW.trigger, label: "Trigger", icon: ZapIcon },
];

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
  return (
    <Nav
      aria-label="Fleet sections"
      className="flex gap-xs overflow-x-auto border-b border-border pb-md"
    >
      {FLEET_NAV_ITEMS.map((item) => {
        const Icon = item.icon;
        const active = item.view === activeView;
        const href = item.view === FLEET_VIEW.chat
          ? baseHref
          : `${baseHref}?view=${item.view}`;
        return (
          <NavItem asChild active={active} key={item.view}>
            <Link href={href}>
              <Icon size={15} />
              {item.label}
            </Link>
          </NavItem>
        );
      })}
    </Nav>
  );
}
