"use client";

import nextDynamic from "next/dynamic";
import type { ComponentProps } from "react";

// Client shim so the create-workspace dialog's chunk loads on demand rather
// than in the shared layout's initial bundle (WorkspaceSwitcher mounts on every
// dashboard page). The dialog is parent-controlled (`open` driven by the
// switcher) and renders nothing while closed, so the loading fallback is
// `null`; the separate "New workspace" trigger lives in the switcher and is
// unaffected. Open/close behaviour and animations are preserved.
const InnerCreateWorkspaceDialog = nextDynamic(
  () =>
    import("@/components/layout/CreateWorkspaceDialog").then((mod) => ({
      default: mod.default,
    })),
  { ssr: false, loading: () => null },
);

export default function CreateWorkspaceDialogDynamic(
  props: ComponentProps<typeof InnerCreateWorkspaceDialog>,
) {
  return <InnerCreateWorkspaceDialog {...props} />;
}
