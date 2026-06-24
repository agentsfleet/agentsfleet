"use client";

import nextDynamic from "next/dynamic";
import type { ComponentProps } from "react";
import { Skeleton } from "@agentsfleet/design-system";

// Client shim for the create-API-key dialog (react-hook-form + zod). Unlike the
// parent-controlled dialogs, this component owns its own trigger button, so the
// loading fallback reserves the trigger's footprint with a button-sized
// Skeleton to avoid a layout shift while the chunk loads after hydration. Keeps
// the dialog body out of the /settings/api-keys initial bundle.
const InnerCreateApiKeyDialog = nextDynamic(
  () =>
    import(
      "@/app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog"
    ).then((mod) => ({ default: mod.default })),
  { ssr: false, loading: () => <Skeleton className="h-9 w-32 rounded-md" /> },
);

export default function CreateApiKeyDialogDynamic(
  props: ComponentProps<typeof InnerCreateApiKeyDialog>,
) {
  return <InnerCreateApiKeyDialog {...props} />;
}
