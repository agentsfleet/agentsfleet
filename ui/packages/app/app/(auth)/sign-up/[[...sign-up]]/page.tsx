import { AuthSignUp } from "@/lib/auth/client";
import { AuthFormLoading } from "@/app/(auth)/AuthFormLoading";

export default function SignUpPage() {
  return <AuthSignUp fallback={<AuthFormLoading kind="sign-up" />} />;
}
