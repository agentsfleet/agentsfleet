import type { BillingSearch } from "./lib/report-query";
import { Suspense } from "react";
import { requireCredential } from "@/lib/auth/credential";
import { RouteLoading } from "@/components/layout/RouteLoading";
import { UsageLayout } from "./components/UsageLayout";
import { UsageReport } from "./components/UsageReport";

export const dynamic = "force-dynamic";

/** The frame streams first; only the data region waits for the report. */
export default async function BillingSettingsPage({
  searchParams,
}: {
  searchParams?: Promise<BillingSearch>;
} = {}) {
  const query = searchParams ? await searchParams : {};
  const token = await requireCredential();
  return <UsageLayout>
    <Suspense fallback={<RouteLoading />}><UsageReport token={token} query={query} /></Suspense>
  </UsageLayout>;
}
