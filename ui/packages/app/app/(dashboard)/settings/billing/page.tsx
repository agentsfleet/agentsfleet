import { PageHeader, PageLayout, PageTitle, Section, SectionHeader } from "@agentsfleet/design-system";
import { requireCredential } from "@/lib/auth/credential";
import { getModelLibrary } from "@/lib/api/model_library";
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

  // The ledger must load. Catalogue failure preserves exact model identifiers.
  const [billing, chargesResp, catalogue] = await Promise.all([
    getTenantBilling(token),
    // The ledger page comes from the URL, so a reload or a shared link opens
    // the page the operator meant rather than resetting to the newest.
    listTenantBillingCharges(token, {
      limit: pageSize,
      ...(cursor ? { cursor } : {}),
    }),
    getModelLibrary(token).catch(() => null),
  ]);

  const names = new Map(catalogue?.models.map(model => [model.id, model.display_name]));
  const charges = chargesResp.items.map(charge => ({...charge, model_display_name: names.get(charge.model)}));

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
