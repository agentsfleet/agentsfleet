"use client";

import type { ComponentProps } from "react";
import { useMemo } from "react";
import { useModelLabels } from "@/components/domain/models/ModelLabels";
import { BillingReportView } from "./BillingReportView";

type Props = Omit<ComponentProps<typeof BillingReportView>, "modelNames">;

/**
 * The report view, named from the dashboard's shared label store. The copy has
 * no prototype, so a model recorded as `constructor` stays a model name.
 */
export function UsageReportView(props: Props) {
  const labels = useModelLabels();
  const modelNames = useMemo(() => Object.assign(Object.create(null) as Record<string, string>, labels), [labels]);
  return <BillingReportView {...props} modelNames={modelNames} />;
}
