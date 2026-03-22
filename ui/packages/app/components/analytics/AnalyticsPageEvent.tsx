"use client";

import { useEffect } from "react";
import { trackAppEvent } from "@/lib/analytics/posthog";

type Props = {
  event: string;
  properties?: Record<string, string | number | boolean | undefined>;
};

export default function AnalyticsPageEvent({ event, properties = {} }: Props) {
  useEffect(() => {
    trackAppEvent(event, properties);
  }, [event, properties]);

  return null;
}
