"use client";

import { useTransition } from "react";
import { useRouter } from "next/navigation";
import { Button } from "@agentsfleet/design-system";

export function RouteRetryButton({ label }: { label: string }) {
  const router = useRouter();
  const [pending, startTransition] = useTransition();
  return <Button variant="secondary" size="sm" disabled={pending} aria-busy={pending}
    onClick={() => startTransition(() => router.refresh())}>
    {pending ? "Retrying…" : label}
  </Button>;
}
