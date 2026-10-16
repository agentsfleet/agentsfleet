import { PageHeader, PageLayout, PageTitle, Section, SectionHeader } from "@agentsfleet/design-system";
import { requireCredential } from "@/lib/auth/credential";
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

export const dynamic = "force-dynamic";

const BILLING_DESCRIPTION = "Your credit balance and a clear record of what your fleets use.";

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
  const token = await requireCredential();

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

  return (
    <PageLayout fullHeight className="h-full overflow-hidden">
      <PageHeader description={BILLING_DESCRIPTION}><PageTitle>Billing</PageTitle></PageHeader>
      <BillingBalanceCard billing={billing} />
      <Section className="flex min-h-0 flex-1 flex-col gap-xl" aria-label="Usage history">
        <SectionHeader as="h2">Usage history</SectionHeader>
        <BillingUsageTab initialCharges={charges} initialCursor={chargesResp.next_cursor} pageSize={pageSize} />
      </Section>
    </PageLayout>
  );
}
