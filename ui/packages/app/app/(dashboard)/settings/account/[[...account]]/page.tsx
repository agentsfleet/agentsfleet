import { redirect } from "next/navigation";
import { ProfileSettings } from "@/components/account/ProfileSettings";

export default async function AccountPage({params}: {params: Promise<{account?: string[]}>}) {
  if ((await params).account?.[0] === "security") redirect("/settings/security");
  return <ProfileSettings />;
}
