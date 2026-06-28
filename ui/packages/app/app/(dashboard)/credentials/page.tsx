import { redirect } from "next/navigation";

// The standalone credentials vault was folded into Models & Keys: the
// custom-secrets section now lives on /settings/models. This route redirects
// there. WORKSPACE_CREDENTIALS_PATH still points at `/credentials`, so
// install-preview service-credential deep-links resolve through this hop without
// coupling to the destination page's layout.
export default function CredentialsPage() {
  redirect("/settings/models");
}
