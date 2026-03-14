import { SignIn } from "@clerk/nextjs";
import { AUTH_APPEARANCE } from "@/lib/clerkAppearance";

export default function SignInPage() {
  return <SignIn appearance={AUTH_APPEARANCE} />;
}
