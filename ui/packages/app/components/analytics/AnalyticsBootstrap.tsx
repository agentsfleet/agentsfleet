"use client";

import { useEffect } from "react";
import { useUser } from "@clerk/nextjs";
import { identifyAnalyticsUser } from "@/lib/analytics/posthog";

export default function AnalyticsBootstrap() {
  const { user, isLoaded } = useUser();

  useEffect(() => {
    if (!isLoaded || !user) return;
    identifyAnalyticsUser({
      id: user.id,
      email: user.primaryEmailAddress?.emailAddress ?? null,
    });
  }, [isLoaded, user]);

  return null;
}
