"use client";

import nextDynamic from "next/dynamic";
import type { ComponentProps } from "react";

// Client shim around `next/dynamic` so the credential-rename dialog's chunk
// stays out of the /secrets route's initial JS bundle. The dialog is
// parent-controlled (`open` is driven by SecretsList) and renders nothing while
// closed, so the loading fallback is `null` — a Skeleton would wrongly paint a
// box where a closed dialog should be invisible. The module loads after
// hydration; the dialog's own open/close (and its exit animation) is unaffected.
const InnerRenameSecretDialog = nextDynamic(
  () =>
    import("@/app/(dashboard)/secrets/components/RenameSecretDialog").then(
      (mod) => ({ default: mod.default }),
    ),
  { ssr: false, loading: () => null },
);

export default function RenameSecretDialogDynamic(
  props: ComponentProps<typeof InnerRenameSecretDialog>,
) {
  return <InnerRenameSecretDialog {...props} />;
}
