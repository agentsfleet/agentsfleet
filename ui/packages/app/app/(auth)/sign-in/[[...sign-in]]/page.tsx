import { AuthSignIn } from "@/lib/auth/client";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";

export default function SignInPage() {
  return <>
    <AuthSignIn appearance={AUTH_APPEARANCE} />
    <p className="max-w-narrow text-center text-sm text-muted-foreground">Forgot your password? Enter your email and continue, then choose <span className="font-medium text-foreground">Forgot password?</span> on the password screen.</p>
  </>;
}
