"use client";

import nextDynamic from "next/dynamic";
import type { ComponentProps } from "react";

// Client shim around `next/dynamic` so the credential-edit dialog's chunk
// (react-hook-form-free but still interaction-only) stays out of the
// /credentials route's initial JS bundle. The dialog is parent-controlled
// (`open` is driven by CredentialsList) and renders nothing while closed, so
// the loading fallback is `null` — a Skeleton would wrongly paint a box where
// a closed dialog should be invisible. The module loads after hydration; the
// dialog's own open/close (and its exit animation) is unaffected.
const InnerEditCredentialDialog = nextDynamic(
  () =>
    import("@/app/(dashboard)/credentials/components/EditCredentialDialog").then(
      (mod) => ({ default: mod.default }),
    ),
  { ssr: false, loading: () => null },
);

export default function EditCredentialDialogDynamic(
  props: ComponentProps<typeof InnerEditCredentialDialog>,
) {
  return <InnerEditCredentialDialog {...props} />;
}
