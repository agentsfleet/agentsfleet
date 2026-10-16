"use client";

import nextDynamic from "next/dynamic";
import { Skeleton } from "@agentsfleet/design-system";

// Readiness and fleet navigation do not need the Markdown parser. Load it
// when instructions are displayed, keeping the same renderer as the chat.
export const FleetMarkdownDynamic = nextDynamic(
  () => import("./FleetMarkdown").then((module) => module.FleetMarkdown),
  { loading: () => <Skeleton className="h-24 w-full" aria-label="Loading instructions" /> },
);
