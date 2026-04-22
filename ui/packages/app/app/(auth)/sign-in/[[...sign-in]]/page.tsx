import { AuthSignIn } from "@/lib/auth/client";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";

export default function SignInPage() {
  return <AuthSignIn appearance={AUTH_APPEARANCE} />;
}
