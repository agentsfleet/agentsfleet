"use client";

import { useEffect } from "react";
import { initAnalytics } from "@/lib/analytics/posthog";

export default function AnalyticsBootstrap() {
  useEffect(() => {
    void initAnalytics();
  }, []);

  return null;
}
