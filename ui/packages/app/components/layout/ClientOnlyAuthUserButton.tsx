"use client";

import { AuthUserButton } from "@/lib/auth/client";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";
import { useMounted } from "@/hooks/use-mounted";

const AUTH_BUTTON_PLACEHOLDER_CLASS = "inline-block h-8 w-8";

export default function ClientOnlyAuthUserButton() {
  const mounted = useMounted();
  if (!mounted) {
    return <span aria-hidden="true" className={AUTH_BUTTON_PLACEHOLDER_CLASS} />;
  }
  return <AuthUserButton appearance={AUTH_APPEARANCE} />;
}
