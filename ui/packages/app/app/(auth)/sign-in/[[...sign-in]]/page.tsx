import { AuthSignIn } from "@/lib/auth/client";
import { AuthFormLoading } from "@/app/(auth)/AuthFormLoading";

export default function SignInPage() {
  return <AuthSignIn fallback={<AuthFormLoading kind="sign-in" />} />;
}
