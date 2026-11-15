"use client";

import type {ComponentProps} from "react";
import {NO_MODEL_LABELS} from "@/lib/model-labels";
import {BillingReportView} from "./BillingReportView";

type Props = Omit<ComponentProps<typeof BillingReportView>, "modelNames">;

/** The table reads shared names inside its own boundary; the report renders immediately. */
export function UsageReportView(props: Props) {
  return <BillingReportView {...props} modelNames={NO_MODEL_LABELS} />;
}
