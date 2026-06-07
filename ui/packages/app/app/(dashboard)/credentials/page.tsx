import { redirect } from "next/navigation";

// Credentials live alongside Models on one unified page now. Keep this route as
// a redirect so existing bookmarks and in-product links land on the Credentials
// section rather than 404. Pre-launch (RULE NLG): a redirect, not a compat shim.
export default function CredentialsPage() {
  redirect("/settings/models#credentials");
}
