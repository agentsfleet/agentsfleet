import { Suspense } from "react";
import { auth } from "@clerk/nextjs/server";
import Link from "next/link";
import { redirect } from "next/navigation";
import {
  Button,
  Card,
  PageHeader,
  PageTitle,
  Section,
  SectionLabel,
  StatusCard,
  Skeleton,
  WakePulse,
} from "@agentsfleet/design-system";
import { listAgents, AGENTSFLEET_STATUS } from "@/lib/api/agents";
import { getTenantBilling } from "@/lib/api/tenant_billing";
import { NANOS_PER_USD } from "@/lib/types";
import { listWorkspaceEvents } from "@/lib/api/events";
import { resolveActiveWorkspace } from "@/lib/workspace";
import { EventsList } from "@/components/domain/EventsList";
import ExhaustionBanner from "@/components/domain/ExhaustionBanner";

export const dynamic = "force-dynamic";

const QUICKSTART_URL = "https://docs.agentsfleet.net/quickstart";

export async function StatusTiles() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) return null;

  const workspace = await resolveActiveWorkspace(token);
  if (!workspace) return null;

  // Request the server max (100) so the Active/Paused/Stopped tiles don't
  // silently under-report for workspaces above the 20-default page size.
  // A dedicated summary endpoint will replace this client-side rollup once it
  // ships; until then 100 matches what the /agents list page uses.
  const [agents, billing] = await Promise.all([
    listAgents(workspace.id, token, { limit: 100 }).then((r) => r.items).catch(() => []),
    getTenantBilling(token).catch(() => null),
  ]);

  const active = agents.filter((z) => z.status === AGENTSFLEET_STATUS.ACTIVE).length;
  const paused = agents.filter((z) => z.status === AGENTSFLEET_STATUS.PAUSED).length;
  const stopped = agents.filter((z) => z.status === AGENTSFLEET_STATUS.STOPPED).length;

  if (agents.length === 0) {
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
    <Section aria-label="Start your fleet" className="mb-8">
      <Card className="overflow-hidden p-0">
        <div className="grid gap-0 lg:grid-cols-2">
          <div className="p-6 sm:p-8">
            <SectionLabel>Next step</SectionLabel>
            <h2 className="mt-3 font-mono text-heading text-foreground">
              Start your fleet
            </h2>
            <p className="mt-3 max-w-prose text-sm text-muted-foreground">
              Start with one <code className="font-mono">SKILL.md</code>. Install it,
              trigger it once, then check Events for the run record.
            </p>
            {credits != null && credits > 0 ? (
              <p className="mt-3 text-sm text-muted-foreground">
                ${credits} free credit is ready for that first run.
              </p>
            ) : null}
            <div className="mt-6 flex flex-wrap gap-3">
              <Button asChild size="sm">
                <Link href="/agents/new">Install teammate</Link>
              </Button>
              <Button asChild variant="ghost" size="sm">
                <a href={QUICKSTART_URL} target="_blank" rel="noopener noreferrer">
                  Quick start
                </a>
              </Button>
            </div>
          </div>
          <FirstRunIllustration />
        </div>
      </Card>
    </Section>
  );
}

function FirstRunIllustration() {
  return (
    <div className="border-t border-border bg-muted/30 p-6 lg:border-l lg:border-t-0">
      <div className="mb-4 flex items-center gap-2 font-mono text-eyebrow uppercase tracking-label text-muted-foreground">
        <WakePulse live className="inline-block h-2.5 w-2.5 rounded-full bg-pulse" aria-hidden="true" />
        First run map
      </div>
      <div className="grid gap-3">
        <IllustrationStep index="1" title="SKILL.md" detail="Behavior and instructions" />
        <IllustrationStep index="2" title="Trigger" detail="Webhook, schedule, or manual wake" />
        <IllustrationStep index="3" title="Evidence" detail="Events and approvals appear here" />
      </div>
    </div>
  );
}

function IllustrationStep({
  index,
  title,
  detail,
}: {
  index: string;
  title: string;
  detail: string;
}) {
  return (
    <div className="flex items-start gap-3 rounded-md border border-border bg-background/40 p-3">
      <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-sm border border-border-strong font-mono text-label text-foreground">
        {index}
      </span>
      <span>
        <span className="block font-mono text-sm text-foreground">{title}</span>
        <span className="block text-xs text-muted-foreground">{detail}</span>
      </span>
    </div>
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
