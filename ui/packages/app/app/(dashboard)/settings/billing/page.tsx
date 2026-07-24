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
import { ReceiptIcon, CreditCardIcon, WalletIcon } from "lucide-react";
import { auth } from "@clerk/nextjs/server";
import { getTenantBilling, listTenantBillingCharges } from "@/lib/api/tenant_billing";
import {
  BILLING_PAGE_SIZE,
  CURSOR_TRAIL_PARAM,
  cursorForTrail,
  cursorTrailFrom,
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
  const cursor = cursorForTrail(cursorTrailFrom(query[CURSOR_TRAIL_PARAM]));
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Fetch in parallel — both endpoints are tenant-scoped (bearer-auth) and
  // independent. getTenantBilling 500s on a tenant whose signup webhook
  // never bootstrapped a billing row; listTenantBillingCharges 503s on a
  // fresh tenant with no events. Catch both so the page renders an
  // explanatory empty state instead of Next's error page.
  const [billing, chargesResp] = await Promise.all([
    getTenantBilling(token).catch(() => null),
    // The ledger page comes from the URL, so a reload or a shared link opens
    // the page the operator meant rather than resetting to the newest.
    listTenantBillingCharges(token, {
      limit: BILLING_PAGE_SIZE,
      ...(cursor ? { cursor } : {}),
    }).catch(() => ({
      items: [],
      next_cursor: null,
    })),
  ]);

  if (!billing) {
    return (
      <PageLayout>
        <PageHeader description={BILLING_DESCRIPTION}>
          <PageTitle>Billing</PageTitle>
        </PageHeader>
        <EmptyState
          icon={<WalletIcon size={28} />}
          title="Billing isn't ready yet"
          description="Refresh in a moment. Contact support if it stays blocked."
        />
      </PageLayout>
    );
  }

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
          <BillingUsageTab initialCharges={charges} initialCursor={chargesResp.next_cursor} />
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
