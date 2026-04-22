"use client";

import { useEffect } from "react";
import { useCurrentUser } from "@/lib/auth/client";
import { identifyAnalyticsUser } from "@/lib/analytics/posthog";

export default function AnalyticsBootstrap() {
  const { isLoaded, isSignedIn, userId, emailAddress } = useCurrentUser();

  useEffect(() => {
    if (!isLoaded || !isSignedIn || !userId) return;
    identifyAnalyticsUser({ id: userId, email: emailAddress });
  }, [isLoaded, isSignedIn, userId, emailAddress]);

  return null;
}
