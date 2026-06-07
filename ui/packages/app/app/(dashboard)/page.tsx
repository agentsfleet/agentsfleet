import { Suspense } from "react";
import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { InstallBlock, PageHeader, PageTitle, Section, SectionLabel, StatusCard, Skeleton } from "@usezombie/design-system";
import { listZombies, ZOMBIE_STATUS } from "@/lib/api/zombies";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { NANOS_PER_USD } from "@/lib/types";
import { listWorkspaceEvents } from "@/lib/api/events";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { AGENT_DEFINITION } from "@/lib/copy";
import { EventsList } from "@/components/domain/EventsList";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";

export const dynamic = "force-dynamic";

export async function StatusTiles() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) return null;

  // Request the server max (100) so the Active/Paused/Stopped tiles don't
  // silently under-report for workspaces above the 20-default page size.
  // A dedicated summary endpoint will replace this client-side rollup once it
  // ships; until then 100 matches what the /zombies list page uses.
  const [zombies, billing] = await Promise.all([
    listZombies(workspace.id, token, { limit: 100 }).then((r) => r.items).catch(() => []),
    getTenantBilling(token).catch(() => null),
  ]);

  const active = zombies.filter((z) => z.status === ZOMBIE_STATUS.ACTIVE).length;
  const paused = zombies.filter((z) => z.status === ZOMBIE_STATUS.PAUSED).length;
  const stopped = zombies.filter((z) => z.status === ZOMBIE_STATUS.STOPPED).length;

  if (zombies.length === 0) {
    return (
      <>
        <ExhaustionBanner billing={billing} />
        <FirstInstallCard balanceNanos={billing?.balance_nanos ?? null} />
      </>
    );
  }

  return (
    <>
      <ExhaustionBanner billing={billing} />
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4 mb-6">
        <StatusCard label="Live" count={active} variant="success" sublabel={active > 0 ? "wake on event" : undefined} />
        <StatusCard label="Paused" count={paused} variant="warning" />
        <StatusCard label="Stopped" count={stopped} variant="default" />
        <StatusCard
          label="Balance"
          count={billing ? `$${(billing.balance_nanos / NANOS_PER_USD).toFixed(2)}` : "—"}
          variant={billing?.is_exhausted ? "danger" : "default"}
        />
      </div>
    </>
  );
}

function FirstInstallCard({ balanceNanos }: { balanceNanos: number | null }) {
  const credits = balanceNanos != null ? Math.floor(balanceNanos / NANOS_PER_USD) : null;
  return (
    <Section aria-label="Install your first agent" className="mb-8">
      <SectionLabel>First wake</SectionLabel>
      <p className="mt-1 mb-3 max-w-prose text-sm text-muted-foreground">
        {AGENT_DEFINITION}
      </p>
      <p className="mb-6 max-w-prose text-sm text-muted-foreground">
        {credits != null && credits > 0
          ? `$${credits} of free credit is sitting in your balance, waiting on a wake. Install an agent from your terminal and trigger one.`
          : "Install an agent from your terminal — point it at a SKILL.md and a TRIGGER.md, and it'll wake on the matching event."}
      </p>
      <InstallBlock
        title="Install your first agent"
        command="zombiectl install --from ./platform-ops"
        actions={[
          { label: "Read the docs", to: "https://docs.usezombie.com/quickstart", variant: "default", external: true },
          { label: "Or paste SKILL.md manually", to: "/zombies/new", variant: "ghost" },
        ]}
      />
    </Section>
  );
}

export async function RecentActivity() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) return null;

  // Dashboard shows a short preview; the full, paginated stream lives at
  // /events (the sidebar "Events" item). Keeps the two from duplicating.
  const page = await listWorkspaceEvents(workspace.id, token, { limit: 5 }).catch(
    () => ({ items: [], next_cursor: null }),
  );

  return (
    <Section asChild>
      <section aria-label="Recent Activity">
        <SectionLabel>Recent Activity</SectionLabel>
        <EventsList
          scope={{ kind: "workspace", workspaceId: workspace.id }}
          initial={page}
          viewAllHref="/events"
        />
      </section>
    </Section>
  );
}

export default async function DashboardPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  return (
    <div>
      <PageHeader>
        <PageTitle>Dashboard</PageTitle>
      </PageHeader>

      <Suspense
        fallback={
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4 mb-6">
            {[0, 1, 2, 3].map((i) => <Skeleton key={i} className="h-20 rounded-lg" />)}
          </div>
        }
      >
        <StatusTiles />
      </Suspense>

      <Suspense fallback={<Skeleton className="h-48 rounded-lg" />}>
        <RecentActivity />
      </Suspense>
    </div>
  );
}
