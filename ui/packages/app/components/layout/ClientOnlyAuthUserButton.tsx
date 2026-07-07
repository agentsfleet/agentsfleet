"use client";

import { useMemo } from "react";
import { AuthUserButton, useCurrentUser } from "@/lib/auth/client";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";
import { avatarGradient } from "@/lib/avatarGradient";
import { useMounted } from "@/hooks/use-mounted";

const AUTH_BUTTON_PLACEHOLDER_CLASS = "inline-block h-8 w-8";

export default function ClientOnlyAuthUserButton() {
  const mounted = useMounted();
  const { userId, emailAddress } = useCurrentUser();
  // DESIGN TOKEN: SKIPPED per user override (reason: per-user deterministic
  // identity pattern — inherently dynamic, computed from the signed-in
  // user's id; no static design token can represent a per-user hash). See
  // docs/DESIGN_SYSTEM.md "Sanctioned non-pulse exception — the account
  // avatar". clerkAppearance.ts's own AUTH_APPEARANCE and its
  // "no decorative gradient on chrome" rule are untouched — this overrides
  // only this one element, at this one call site.
  const appearance = useMemo(
    () => ({
      ...AUTH_APPEARANCE,
      elements: {
        ...AUTH_APPEARANCE.elements,
        userButtonAvatarBox: {
          ...AUTH_APPEARANCE.elements.userButtonAvatarBox,
          background: avatarGradient(userId ?? emailAddress ?? ""),
        },
      },
    }),
    [userId, emailAddress],
  );

  if (!mounted) {
    return <span aria-hidden="true" className={AUTH_BUTTON_PLACEHOLDER_CLASS} />;
  }
  return <AuthUserButton appearance={appearance} />;
}
