import { notFound } from "next/navigation";
import {
  EmptyState,
  PageHeader,
  PageTitle,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@usezombie/design-system";
import { ReceiptIcon, CreditCardIcon } from "lucide-react";
import { getServerToken } from "@/lib/auth/server";
import { getTenantBilling, listTenantBillingCharges } from "@/lib/api/tenant_billing";
import BillingBalanceCard from "./components/BillingBalanceCard";
import BillingUsageTab from "./components/BillingUsageTab";
import { groupChargesByEvent } from "./lib/groupCharges";

export const dynamic = "force-dynamic";

const PURCHASE_FOOTNOTE = "Stripe purchase ships in v2.1.";

export default async function BillingSettingsPage() {
  const token = await getServerToken();
  if (!token) notFound();

  // Fetch in parallel — both endpoints are tenant-scoped (bearer-auth) and
  // independent. listTenantBillingCharges 503s on a fresh tenant with no
  // events; we tolerate that by falling back to an empty items array so the
  // page still renders the balance card.
  const [billing, chargesResp] = await Promise.all([
    getTenantBilling(token),
    listTenantBillingCharges(token, 50).catch(() => ({ items: [] }) as Awaited<
      ReturnType<typeof listTenantBillingCharges>
    >),
  ]);

  const events = groupChargesByEvent(chargesResp.items);

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
          <BillingUsageTab events={events} />
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
