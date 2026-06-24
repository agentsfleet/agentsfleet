"use client";

import nextDynamic from "next/dynamic";
import type { ComponentProps } from "react";

// Client shim for the "Add a custom secret" form. It lives inside a collapsed
// `<details>` disclosure on /credentials, so it is interaction-only — its chunk
// (react-hook-form + zod resolver) has no business in the route's initial
// bundle. The disclosure is empty until opened, so the loading fallback is
// `null`; the module loads after hydration, well before a user expands it.
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
