import { redirect } from "next/navigation";
import { ProfileSettings } from "@/components/account/ProfileSettings";
import { AccountMethodsSettings } from "@/components/account/AccountMethodsSettings";

export default async function AccountPage({params}: {params: Promise<{account?: string[]}>}) {
  const destination = (await params).account?.[0];
  if (destination === "security") redirect("/settings/security");
  if (destination === "sign-in") return <AccountMethodsSettings />;
  return <ProfileSettings />;
}
