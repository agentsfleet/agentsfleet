"use client";

import { useEffect } from "react";
import { usePathname } from "next/navigation";
import { useCurrentUser } from "@/lib/auth/client";
import { isSensitiveAuthUrl, onAuthenticationRoute } from "@/lib/analytics/auth-privacy";
import { initAnalytics } from "@/lib/analytics/posthog";

export default function AnalyticsBootstrap() {
  const pathname = usePathname();
  const { isLoaded, isSignedIn, userId, emailAddress } = useCurrentUser();

  useEffect(() => {
    if (!pathname || isSensitiveAuthUrl(pathname) || onAuthenticationRoute()) return;
    let active = true;
    void initAnalytics(() => {
      if (!active || !isLoaded || window.location.pathname !== pathname || onAuthenticationRoute()) return undefined;
      if (!isSignedIn) return null;
      return userId ? { id: userId, email: emailAddress } : undefined;
    }).catch(() => {
      // External analytics failures must not interrupt authentication or navigation.
    });
    return () => { active = false; };
  }, [pathname, isLoaded, isSignedIn, userId, emailAddress]);

  return null;
}
