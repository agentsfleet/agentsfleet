import { redirect } from "next/navigation";
import {
  EmptyState,
  PageHeader,
  PageLayout,
  PageTitle,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@agentsfleet/design-system";
import { ReceiptIcon, CreditCardIcon } from "lucide-react";
import { auth } from "@clerk/nextjs/server";
import {
  getTenantBilling,
  listTenantBillingCharges,
} from "@/lib/api/tenant_billing";
import {
  CURSOR_PAGE_SIZE_PARAM,
  CURSOR_TRAIL_PARAM,
  PAGE_SIZE_PARAM,
  cursorForTrail,
  cursorTrailFrom,
  pageSizeFrom,
} from "@/lib/pagination/cursor-trail";
import BillingBalanceCard from "./components/BillingBalanceCard";
import BillingUsageTab from "./components/BillingUsageTab";
import { summarizeCharges } from "./lib/charges";

export const dynamic = "force-dynamic";

const BILLING_DESCRIPTION = "Manage credits and usage.";

export default async function BillingSettingsPage({
  searchParams,
}: {
  searchParams?: Promise<Record<string, string | string[] | undefined>>;
} = {}) {
  const query = searchParams ? await searchParams : {};
  const pageSize = pageSizeFrom(query[PAGE_SIZE_PARAM]);
  const cursor = cursorForTrail(
    cursorTrailFrom(
      query[CURSOR_TRAIL_PARAM],
      pageSize,
      query[CURSOR_PAGE_SIZE_PARAM],
    ),
  );
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Both reads are independent. Failures reach the shared retry boundary so an
  // unavailable ledger never becomes a successful "No charges yet" result.
  const [billing, chargesResp] = await Promise.all([
    getTenantBilling(token),
    // The ledger page comes from the URL, so a reload or a shared link opens
    // the page the operator meant rather than resetting to the newest.
    listTenantBillingCharges(token, {
      limit: pageSize,
      ...(cursor ? { cursor } : {}),
    }),
  ]);

  const charges = chargesResp.items;
  const summary = summarizeCharges(charges, billing.balance_nanos);

  return (
    <PageLayout>
      <PageHeader description={BILLING_DESCRIPTION}>
        <PageTitle>Billing</PageTitle>
      </PageHeader>

      <BillingBalanceCard billing={billing} summary={summary} />

      <Tabs defaultValue="usage">
        <TabsList>
          <TabsTrigger value="usage">Usage</TabsTrigger>
          <TabsTrigger value="invoices">Invoices</TabsTrigger>
          <TabsTrigger value="payment">Payment method</TabsTrigger>
        </TabsList>

        <TabsContent value="usage" className="mt-4 space-y-6">
          <BillingUsageTab
            initialCharges={charges}
            initialCursor={chargesResp.next_cursor}
            pageSize={pageSize}
          />
        </TabsContent>

        <TabsContent value="invoices" className="mt-4">
          <EmptyState
            icon={<ReceiptIcon size={28} />}
            title="No invoices yet"
            description="Your invoices will appear here."
          />
        </TabsContent>

        <TabsContent value="payment" className="mt-4">
          <EmptyState
            icon={<CreditCardIcon size={28} />}
            title="No payment method on file"
            description="Your saved payment methods will appear here."
          />
        </TabsContent>
      </Tabs>
    </PageLayout>
  );
}
