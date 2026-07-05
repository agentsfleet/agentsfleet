import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";

// The Workspace tab folded into API Keys (workspace name/ID now lives there,
// switching/creating stays in the top-right WorkspaceSwitcher) — this route
// survives only so existing "/settings" links and bookmarks keep resolving.
// Checks auth itself (rather than deferring to /settings/api-keys) so an
// unauthenticated visit goes straight to /sign-in instead of a double redirect.
export default async function SettingsPage() {
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");
  redirect("/settings/api-keys");
}
