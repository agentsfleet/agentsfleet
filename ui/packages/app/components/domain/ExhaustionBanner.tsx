import { AlertTriangleIcon } from "lucide-react";
import { Alert, AlertTitle, AlertDescription } from "@usezombie/design-system";
import type { TenantBilling } from "@/lib/types";

type Props = { billing: TenantBilling | null };

export default function ExhaustionBanner({ billing }: Props) {
  if (!billing?.is_exhausted) return null;
  const when = billing.exhausted_at
    ? new Date(billing.exhausted_at).toLocaleString()
    : null;
  return (
    <Alert variant="destructive" className="mb-6">
      <AlertTriangleIcon size={18} className="mt-0.5 shrink-0" aria-hidden />
      <div className="min-w-0 flex-1">
        <AlertTitle>Your credit balance is exhausted.</AlertTitle>
        <AlertDescription className="text-destructive/80">
          New zombie runs follow the server{"'"}s{" "}
          <code className="font-mono text-xs">BALANCE_EXHAUSTED_POLICY</code>{" "}
          (continue, warn, or stop).{" "}
          {when ? <>Exhausted since {when}. </> : null}
          <a
            href="mailto:support@usezombie.com"
            className="underline underline-offset-2 hover:text-destructive"
          >
            Contact support
          </a>{" "}
          to top up.
        </AlertDescription>
      </div>
    </Alert>
  );
}
