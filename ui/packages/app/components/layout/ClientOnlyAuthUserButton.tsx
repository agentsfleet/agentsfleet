"use client";

import { useMemo } from "react";
import { AuthUserButton, useCurrentUser } from "@/lib/auth/client";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";
import { avatarColor } from "@/lib/avatarColor";
import { useMounted } from "@/hooks/use-mounted";

const AUTH_BUTTON_PLACEHOLDER_CLASS = "inline-block h-8 w-8";

export default function ClientOnlyAuthUserButton() {
  const mounted = useMounted();
  const { userId, emailAddress, hasImage } = useCurrentUser();
  const appearance = useMemo(
    () => ({
      ...AUTH_APPEARANCE,
      elements: {
        ...AUTH_APPEARANCE.elements,
        userButtonAvatarBox: {
          ...AUTH_APPEARANCE.elements.userButtonAvatarBox,
          background: avatarColor(userId ?? emailAddress ?? ""),
        },
        ...(!hasImage ? {
          avatarImage: { display: "none" },
          avatarBox: {
            position: "relative",
            background: avatarColor(userId ?? emailAddress ?? ""),
            "&::before": {
              content: '""',
              position: "absolute",
              inset: "15%",
              backgroundColor: "var(--text)",
              mask: 'url("/user.svg") center / contain no-repeat',
            },
          },
        } as const : {}),
      },
    }),
    [userId, emailAddress, hasImage],
  );

  if (!mounted) {
    return <span aria-hidden="true" className={AUTH_BUTTON_PLACEHOLDER_CLASS} />;
  }
  return <AuthUserButton appearance={appearance} userProfileMode="navigation" userProfileUrl="/settings/account" />;
}
