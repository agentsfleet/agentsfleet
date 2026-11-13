import { redirect } from "next/navigation";
import { requireCredential } from "@/lib/auth/credential";

export default async function SettingsSecurityPage(_props: {params: Promise<{workspaceId: string}>}) {
  await requireCredential();
  redirect("/settings/security");
}
