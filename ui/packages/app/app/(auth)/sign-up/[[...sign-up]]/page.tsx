import { SignUp } from "@clerk/nextjs";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";

export default function SignUpPage() {
  return <SignUp appearance={AUTH_APPEARANCE} />;
}
