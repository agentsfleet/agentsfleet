import type { ReactNode } from "react";
import { ModelLabelsProvider } from "@/components/domain/models/ModelLabels";
import { requireCredential } from "@/lib/auth/credential";
import { NO_MODEL_LABELS } from "@/lib/model-labels";
import { modelLabelsCached } from "@/lib/model-labels-server";

/** The spending table waits for model names while the rest of Usage streams. */
export default async function BillingLayout({children}: {children: ReactNode}) {
  const token = await requireCredential();
  const labels = modelLabelsCached(token).catch(() => NO_MODEL_LABELS);
  return <ModelLabelsProvider labels={NO_MODEL_LABELS} pending={labels}>{children}</ModelLabelsProvider>;
}
