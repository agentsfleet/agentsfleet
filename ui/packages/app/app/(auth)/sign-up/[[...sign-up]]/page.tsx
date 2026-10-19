import Link from "next/link";
import { AuthSignUp } from "@/lib/auth/client";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";

export default function SignUpPage() {
  return <>
    <AuthSignUp appearance={AUTH_APPEARANCE} />
    <p className="max-w-narrow text-center text-sm text-muted-foreground">Already have an account? <Link href="/sign-in" className="font-medium text-primary underline underline-offset-4">Sign in to reset your password</Link>.</p>
  </>;
}
