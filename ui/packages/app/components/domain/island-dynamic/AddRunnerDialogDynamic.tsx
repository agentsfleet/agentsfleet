"use client";

import nextDynamic from "next/dynamic";
import type { ComponentProps } from "react";
import { Skeleton } from "@agentsfleet/design-system";

// Client shim for the add-runner dialog (react-hook-form + zod + the sandbox
// tier select). It owns its own trigger button, so the loading fallback
// reserves the trigger footprint with a button-sized Skeleton to avoid a layout
// shift while the chunk loads after hydration. Keeps the dialog body out of the
// /admin/runners initial bundle.
const InnerAddRunnerDialog = nextDynamic(
  () =>
    import("@/app/(dashboard)/admin/runners/components/AddRunnerDialog").then(
      (mod) => ({ default: mod.default }),
    ),
  { ssr: false, loading: () => <Skeleton className="h-9 w-32 rounded-md" /> },
);

export default function AddRunnerDialogDynamic(
  props: ComponentProps<typeof InnerAddRunnerDialog>,
) {
  return <InnerAddRunnerDialog {...props} />;
}
