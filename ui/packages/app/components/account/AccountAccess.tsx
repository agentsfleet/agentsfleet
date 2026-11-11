"use client";

import Link from "next/link";
import { Button, EmptyState } from "@agentsfleet/design-system";
import { RouteLoading } from "@/components/layout/RouteLoading";

export function AccountAccess({status}: {status: "loading" | "signed-out"}) {
  return status === "loading" ? <RouteLoading /> : <EmptyState title="Sign in to manage your account"
    description="Your profile and browser sessions are available after signing in."
    action={<Button asChild><Link href="/sign-in">Sign in</Link></Button>} />;
}
