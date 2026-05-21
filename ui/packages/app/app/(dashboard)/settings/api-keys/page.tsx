import { redirect } from "next/navigation";
import { PageHeader, PageTitle, Section } from "@usezombie/design-system";
import { auth } from "@clerk/nextjs/server";
import { ApiError } from "@/lib/api/errors";
import { listApiKeys, DEFAULT_PAGE_SIZE, DEFAULT_SORT } from "@/lib/api/api_keys";
import ApiKeyList from "./components/ApiKeyList";

export const dynamic = "force-dynamic";

export default async function ApiKeysPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // RBAC guard via defense-in-depth: the dashboard session token carries no
  // role claim (AUTH.md — role lives only in the api-template token the backend
  // verifies), so we mirror route_table.zig's operator() policy by letting the
  // backend arbitrate. A `user`-role principal gets 403; redirect to /settings.
  let data;
  try {
    data = await listApiKeys(token, { page: 1, page_size: DEFAULT_PAGE_SIZE, sort: DEFAULT_SORT });
  } catch (e) {
    if (e instanceof ApiError && e.status === 403) redirect("/settings?notice=api-keys-operator-only");
    if (e instanceof ApiError && e.status === 401) redirect("/sign-in");
    throw e;
  }

  return (
    <div>
      <PageHeader>
        <PageTitle>API keys</PageTitle>
      </PageHeader>
      <p className="mb-6 text-sm text-muted-foreground">
        Tenant API keys (<code>zmb_t_…</code>) authenticate service-to-service callers — n8n, Zapier,
        cron, CI. The raw key is shown <strong>once</strong> at creation; store it somewhere safe.
        Revoke a key to disable it immediately; delete a revoked key to remove the record.
      </p>
      <Section asChild>
        <section aria-label="API keys">
          <ApiKeyList initial={data} />
        </section>
      </Section>
    </div>
  );
}
