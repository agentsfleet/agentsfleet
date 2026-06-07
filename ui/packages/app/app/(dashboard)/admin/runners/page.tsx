import { redirect } from "next/navigation";
import { auth } from "@clerk/nextjs/server";
import { ApiError } from "@/lib/api/errors";
import { readPlatformAdminClaim } from "@/lib/auth/platform";
import { listRunners, DEFAULT_PAGE_SIZE, DEFAULT_SORT } from "@/lib/api/runners";
import RunnersView from "./components/RunnersView";

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

  return <RunnersView initial={data} />;
}
