import type { ReactNode } from "react";
import type { ConnectorCatalogEntry } from "@/lib/api/connectors";
import { useEffect, useState } from "react";
import { AppRouterContext } from "next/dist/shared/lib/app-router-context.shared-runtime";
import { PathnameContext, SearchParamsContext } from "next/dist/shared/lib/hooks-client-context.shared-runtime";
import { Button, PageHeader, PageLayout, PageTitle, Section, SectionHeader, TooltipProvider } from "@agentsfleet/design-system";
import { ShellFrame } from "@/components/layout/ShellFrame";
import { LibraryWorkspace } from "@/components/domain/chat/LibraryWorkspace";
import { DraftConversation } from "@/components/domain/chat/DraftConversation";
import { DraftChatHeading } from "@/components/domain/chat/DraftChatHeading";
import { DraftClient } from "@/lib/api/drafts";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { SUPPORT_EMAIL } from "@/lib/contact";
import SecretsList from "@/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList";
import AddSecretDialog from "@/app/(dashboard)/w/[workspaceId]/secrets/components/AddSecretDialog";
import { SecretsDescription } from "@/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsDescription";
import IntegrationsConnectors from "@/app/(dashboard)/w/[workspaceId]/integrations/components/IntegrationsConnectors";
import { CONNECT_APPS_COPY } from "@/app/(dashboard)/w/[workspaceId]/integrations/copy";
import ApiKeysView from "@/app/(dashboard)/settings/api-keys/components/ApiKeysView";
import { BillingReportView } from "@/app/(dashboard)/settings/billing/components/BillingReportView";
import { BILLING_DETAIL_STATE } from "@/app/(dashboard)/settings/billing/lib/report-detail";
import { BILLING_MODEL, BILLING_MODEL_NAME, billingReport } from "@/tests/helpers/billing-report";
import { OWN_ACCOUNT } from "@/tests/helpers/workspace-fixtures";
import { MODEL, WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { LIBRARY_PAGE_DESCRIPTION, LIBRARY_PAGE_TITLE } from "@/app/(dashboard)/w/[workspaceId]/library/copy";

const router = {bfcacheId: "resource-pages-review", back() {}, forward() {}, refresh() {}, push() {}, replace() {}, async prefetch() {}};
const PAGES = [
  {key: "usage", path: "/settings/billing", label: "Usage"},
  {key: "secrets", path: `/w/${WORKSPACE}/secrets`, label: "Secrets"},
  {key: "library", path: `/w/${WORKSPACE}/library`, label: "Plugins"},
  {key: "integrations", path: `/w/${WORKSPACE}/integrations`, label: "Connected apps"},
  {key: "keys", path: "/settings/api-keys", label: "API keys"},
  {key: "new", path: `/w/${WORKSPACE}/fleets/new`, label: "New fleet"},
] as const;
const CONNECTORS: ConnectorCatalogEntry[] = [
  {id: "github", display_name: "GitHub", archetype: "app_install", configured: true, connected: true, label: "Release workspace"},
  {id: "slack", display_name: "Slack", archetype: "oauth2", configured: true, connected: false, label: null},
];

/** Production components and page composition; explicit samples, no live writes. */
export function ResourcePagesFixture() {
  const [selected, setSelected] = useState<typeof PAGES[number]>(PAGES[0]);
  const workspaces = [{id: WORKSPACE, name: "Release workspace", created_at: "2026-01-01T00:00:00Z", ...OWN_ACCOUNT}];
  return <AppRouterContext value={router}><PathnameContext value={selected.path}><SearchParamsContext value={new URLSearchParams()}><TooltipProvider>
    <style>{".app-shell{top:44px;height:calc(100dvh - 44px)}"}</style>
    <div data-review-pages className="fixed inset-x-0 top-0 z-50 flex h-11 items-center gap-sm overflow-x-auto whitespace-nowrap bg-surface-deep px-lg [&>button]:shrink-0">
      <span className="mr-auto text-eyebrow text-muted-foreground">Component review · Sample data · No live writes</span>
      {PAGES.map(page => <Button key={page.key} size="sm" variant="ghost" onClick={() => setSelected(page)}>{page.label}</Button>)}
    </div>
    <ShellFrame workspaces={workspaces}><ResourcePage kind={selected.key} /></ShellFrame>
  </TooltipProvider></SearchParamsContext></PathnameContext></AppRouterContext>;
}

function ResourcePage({kind}: {kind: typeof PAGES[number]["key"]}) {
  if (kind === "keys") return <ApiKeysView operatorOnly={false} initial={{items: [{id: "sample-key", key_name: "Release tooling", active: true, created_at: "2026-11-01T10:00:00Z", revoked_at: null}], total: 1, next_cursor: null}} />;
  if (kind === "new") return <EmptySetup />;
  if (kind === "usage") return <PageLayout className="billing-page gap-lg font-sans [&_button]:font-sans">
    <PageHeader className="min-h-9 flex-row items-center gap-lg border-0 pb-0 sm:gap-lg" actions={<Button size="sm" asChild><a href={`mailto:${SUPPORT_EMAIL}`}>Add credits</a></Button>}><PageTitle className="font-sans text-heading font-medium">Usage</PageTitle></PageHeader>
    <BillingReportView report={billingReport()} pageSize={25} modelNames={{[BILLING_MODEL]: BILLING_MODEL_NAME}} detail={{status: BILLING_DETAIL_STATE.closed}} selected="" />
  </PageLayout>;
  if (kind === "library") return <Resource title={LIBRARY_PAGE_TITLE} description={LIBRARY_PAGE_DESCRIPTION}><LibraryWorkspace workspace={WORKSPACE} /></Resource>;
  if (kind === "integrations") return <Resource title={CONNECT_APPS_COPY.title} description={CONNECT_APPS_COPY.description}><IntegrationsConnectors workspaceId={WORKSPACE} catalog={CONNECTORS} /></Resource>;
  return <Resource title="Secrets" description={<SecretsDescription />}><Section aria-label="Stored secrets" className="flex flex-col gap-xl">
    <SectionHeader as="p" actions={<AddSecretDialog workspaceId={WORKSPACE} />}>Stored secrets</SectionHeader>
    <SecretsList workspaceId={WORKSPACE} secrets={[{name: "Release service", kind: "custom_secret", created_at: "2026-11-11T10:00:00Z"}]} />
  </Section></Resource>;
}

function Resource({title, description, children}: {title: string; description?: ReactNode; children: ReactNode}) {
  return <PageLayout><PageHeader description={description}><PageTitle>{title}</PageTitle></PageHeader>{children}</PageLayout>;
}

function EmptySetup() {
  const [client] = useState(() => new DraftClient({account: "fixture-owner", workspace: WORKSPACE, subject: "creator"}));
  useEffect(() => () => client.dispose(), [client]);
  const view = {...EMPTY_DRAFT_VIEW, model: MODEL.ref, models: [MODEL]};
  return <PageLayout fullHeight className="flex min-h-0 flex-1 flex-col"><div data-fleet-authoring className="flex min-h-0 flex-1 flex-col">
    <DraftChatHeading workspace={WORKSPACE} client={client} view={view} onDirtyChange={() => {}} />
    <DraftConversation client={client} view={view} workspace={WORKSPACE} />
  </div></PageLayout>;
}
