"use client";

import type { ReactNode } from "react";
import { useAuthLoadFailed } from "@/lib/auth/client";

type Props = {children: ReactNode; failure: ReactNode};

export function AuthFormStatus({children, failure}: Props) {
  return useAuthLoadFailed() ? failure : children;
}
