"use client";

import type { EventsListProps } from "@/components/domain/EventsList";
import nextDynamic from "next/dynamic";

// Activity keeps server rendering; its table runtime stays out of the Chat entry.
const ActivityTable = nextDynamic(() => import("@/components/domain/EventsList").then(module => module.EventsList));

export function FleetActivity(props: EventsListProps) {
  return <ActivityTable {...props} />;
}
