import { redirect } from "next/navigation";
import { PageHeader, PageTitle, Section } from "@usezombie/design-system";
import { auth } from "@clerk/nextjs/server";
import { ApiError } from "@/lib/api/errors";
import { readPlatformAdminClaim } from "@/lib/auth/platform";
import { listRunners, DEFAULT_PAGE_SIZE, DEFAULT_SORT } from "@/lib/api/runners";
import RunnerList from "./components/RunnerList";

export const dynamic = "force-dynamic";

const NOT_ADMIN = "/settings?notice=runners-platform-admin-only";

export default async function RunnersPage() {
  // Platform-admin only — hide the surface entirely for everyone else. The
  // backend independently 403s a non-admin (UZ-AUTH-021); this is the UI guard.
  if (!(await readPlatformAdminClaim())) redirect(NOT_ADMIN);

  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  let data;
  try {
    data = await listRunners(token, { page: 1, page_size: DEFAULT_PAGE_SIZE, sort: DEFAULT_SORT });
  } catch (e) {
    if (e instanceof ApiError && e.status === 403) redirect(NOT_ADMIN);
    if (e instanceof ApiError && e.status === 401) redirect("/sign-in");
    throw e;
  }

  return (
    <div>
      <PageHeader>
        <PageTitle>Runners</PageTitle>
      </PageHeader>
      <p className="mb-6 text-sm text-muted-foreground">
        Enroll a host into the shared fleet. <strong>Add runner</strong> mints a runner token
        (<code>zrn_…</code>) shown <strong>once</strong> — install it on the host as{" "}
        <code>ZOMBIE_RUNNER_TOKEN</code>; the host never holds your identity credential. A freshly
        minted runner shows <strong>registered</strong> until it first connects.
      </p>
      <Section asChild>
        <section aria-label="Runners">
          <RunnerList initial={data} />
        </section>
      </Section>
    </div>
  );
}
