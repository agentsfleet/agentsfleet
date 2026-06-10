"use client";

import { useEffect } from "react";
import { useCurrentUser } from "@/lib/auth/client";
import {
  hasStaleAnalyticsIdentity,
  identifyAnalyticsUser,
  resetAnalyticsIdentity,
} from "@/lib/analytics/posthog";

export default function AnalyticsBootstrap() {
  const { isLoaded, isSignedIn, userId, emailAddress } = useCurrentUser();

  useEffect(() => {
    if (!isLoaded) return;
    if (isSignedIn) {
      if (userId) identifyAnalyticsUser({ id: userId, email: emailAddress });
      return;
    }
    // Signed-out while a prior identity lingers (sign-out edge, hard
    // navigation, or session expiry): clear it exactly once so the next
    // anonymous/other session does not stitch to the previous distinct_id.
    // Anonymous visitors never carry the marker, so this never churns
    // anonymous ids.
    if (hasStaleAnalyticsIdentity()) resetAnalyticsIdentity();
  }, [isLoaded, isSignedIn, userId, emailAddress]);

  return null;
}
