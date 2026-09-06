import { redirect } from "next/navigation";
import { auth } from "@clerk/nextjs/server";
import { ApiError } from "@/lib/api/errors";
import { hasScope } from "@/lib/auth/platform";
import { SCOPE } from "@/lib/auth/scopes";
import { listAdminModels, listPlatformKeys, activePlatformDefault, type PlatformKey } from "@/lib/api/admin_model_library";
import ModelsView from "./components/ModelsView";

export const dynamic = "force-dynamic";

const NOT_ADMIN = "/settings?notice=models-platform-admin-only";

export default async function AdminModelsPage() {
  // Model operators only — hide the surface for a token without `model:read`.
  // The backend independently 403s a token missing the scope (UZ-AUTH-022);
  // this is the UI guard.
  if (!(await hasScope(SCOPE.MODEL_READ))) redirect(NOT_ADMIN);

  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  // Both reads depend only on the token, so they run together. The catalogue
  // decides the page; the active platform default only badges a catalogue row —
  // a non-essential indicator whose GET is gated on platform-key:read (a
  // distinct scope from this page's model:read), so a model:read-only viewer
  // 403s there, and a transient failure is likewise possible. Either way the
  // badge degrades to "no default known" rather than failing the page.
  const [catalogue, platformKeys] = await Promise.allSettled([
    listAdminModels(token),
    listPlatformKeys(token),
  ]);
  if (catalogue.status === "rejected") {
    const e: unknown = catalogue.reason;
    if (e instanceof ApiError && e.status === 403) redirect(NOT_ADMIN);
    if (e instanceof ApiError && e.status === 401) redirect("/sign-in");
    throw e;
  }
  const activeDefault: PlatformKey | null =
    platformKeys.status === "fulfilled" ? activePlatformDefault(platformKeys.value) : null;

  return <ModelsView initial={catalogue.value} activeDefault={activeDefault} />;
}
