"use client";

import nextDynamic from "next/dynamic";
import type { ComponentProps } from "react";

// Client shim for the "Add a custom secret" form. It sits in the Credentials
// vault's terminal-style add row, so the route keeps react-hook-form + zod out
// of the server bundle and hydrates the form where interaction begins.
const InnerAddCredentialForm = nextDynamic(
  () =>
    import("@/app/(dashboard)/credentials/components/AddCredentialForm").then(
      (mod) => ({ default: mod.default }),
    ),
  { ssr: false, loading: () => null },
);

export default function AddCredentialFormDynamic(
  props: ComponentProps<typeof InnerAddCredentialForm>,
) {
  return <InnerAddCredentialForm {...props} />;
}
