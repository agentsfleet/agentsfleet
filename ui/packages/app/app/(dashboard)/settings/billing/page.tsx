import { redirect } from "next/navigation";
import {
  EmptyState,
  PageHeader,
  PageTitle,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@usezombie/design-system";
import { ReceiptIcon, CreditCardIcon, WalletIcon } from "lucide-react";
import { auth } from "@clerk/nextjs/server";
import { getTenantBilling, listTenantBillingCharges } from "@/lib/api/tenant_billing";
import BillingBalanceCard from "./components/BillingBalanceCard";
import BillingUsageTab from "./components/BillingUsageTab";
import { groupChargesByEvent } from "./lib/groupCharges";

export const dynamic = "force-dynamic";

const PURCHASE_FOOTNOTE = "Stripe purchase ships in v2.1.";

export default async function BillingSettingsPage() {
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
    listTenantBillingCharges(token, { limit: 50 }).catch(() => ({
      items: [],
      next_cursor: null,
    })),
  ]);

  if (!billing) {
    return (
      <div className="space-y-8">
        <PageHeader>
          <PageTitle>Billing</PageTitle>
        </PageHeader>
        <EmptyState
          icon={<WalletIcon size={28} />}
          title="Billing isn't ready yet"
          description="Your tenant is still being set up. Refresh in a moment; if this persists, contact support."
        />
      </div>
    );
  }

  const events = groupChargesByEvent(chargesResp.items);
  const initialCursor = chargesResp.next_cursor;

  return (
    <div className="space-y-8">
      <PageHeader>
        <PageTitle>Billing</PageTitle>
      </PageHeader>

      <BillingBalanceCard billing={billing} />

      <Tabs defaultValue="usage" className="max-w-5xl">
        <TabsList>
          <TabsTrigger value="usage">Usage</TabsTrigger>
          <TabsTrigger value="invoices">Invoices</TabsTrigger>
          <TabsTrigger value="payment">Payment Method</TabsTrigger>
        </TabsList>

        <TabsContent value="usage" className="mt-4">
          <BillingUsageTab initialEvents={events} initialCursor={initialCursor} />
        </TabsContent>

        <TabsContent value="invoices" className="mt-4">
          <EmptyState
            icon={<ReceiptIcon size={28} />}
            title="No invoices yet"
            description={`Invoicing arrives with Purchase Credits in v2.1. ${PURCHASE_FOOTNOTE}`}
          />
        </TabsContent>

        <TabsContent value="payment" className="mt-4">
          <EmptyState
            icon={<CreditCardIcon size={28} />}
            title="No payment method on file"
            description={`Payment methods arrive with Purchase Credits in v2.1. ${PURCHASE_FOOTNOTE}`}
          />
        </TabsContent>
      </Tabs>
    </div>
  );
}
