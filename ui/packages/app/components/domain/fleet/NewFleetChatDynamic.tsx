"use client";

import nextDynamic from "next/dynamic";
import { RouteLoading } from "@/components/layout/RouteLoading";

// Client-Component shim around `next/dynamic`, as `FleetThreadDynamic` is for a
// fleet's chat: the New fleet route stays a Server Component, and the
// assistant-ui chunk the composer needs loads apart from the route's bundle.

const NewFleetChatDynamic = nextDynamic(() => import("./NewFleetChat").then(module => module.NewFleetChat), {
  ssr: false,
  loading: () => <RouteLoading label="Loading new fleet" />,
});

export default NewFleetChatDynamic;
