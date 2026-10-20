import { AuthSignUp } from "@/lib/auth/client";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";

export default function SignUpPage() {
  return <AuthSignUp appearance={AUTH_APPEARANCE} />;
}
