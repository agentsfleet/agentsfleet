"use client";

import nextDynamic from "next/dynamic";
import type { ComponentProps } from "react";
import { Skeleton } from "@agentsfleet/design-system";

// Client shim for the edit-policy dialog — the same react-hook-form + zod +
// design-system form stack the add-runner dialog carries, and the same reason
// for the island: on the runner detail route that stack is 33 kB of critical
// path for a control most page views never open. It owns its own trigger
// button, so the loading fallback reserves the trigger footprint with a
// button-sized Skeleton (h-8 = the `size="sm"` Button height) to avoid a
// layout shift while the chunk loads after hydration.
const InnerEditPolicyDialog = nextDynamic(
  () =>
    import("@/app/(dashboard)/admin/runners/components/EditPolicyDialog").then(
      (mod) => ({ default: mod.EditPolicyDialog }),
    ),
  { ssr: false, loading: () => <Skeleton className="h-8 w-28 rounded-md" /> },
);

export default function EditPolicyDialogDynamic(
  props: ComponentProps<typeof InnerEditPolicyDialog>,
) {
  return <InnerEditPolicyDialog {...props} />;
}
