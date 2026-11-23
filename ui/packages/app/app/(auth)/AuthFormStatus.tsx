"use client";

import type { ComponentProps, ReactNode } from "react";
import { useAuthLoadFailed } from "@/lib/auth/client";

type Props = {children: ReactNode; failure: ReactNode};

export function AuthFormStatus({children, failure}: Props) {
  return useAuthLoadFailed() ? failure : children;
}

export function AuthFormReload(props: ComponentProps<"button">) {
  return <button {...props} type="button" onClick={() => window.location.reload()}>Reload page</button>;
}
