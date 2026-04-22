import { AlertTriangleIcon } from "lucide-react";
import type { TenantBilling } from "@/lib/types";

type Props = { billing: TenantBilling | null };

export default function ExhaustionBanner({ billing }: Props) {
  if (!billing?.is_exhausted) return null;
  const when = billing.exhausted_at
    ? new Date(billing.exhausted_at).toLocaleString()
    : null;
  return (
    <div
      role="alert"
      className="mb-6 flex items-start gap-3 rounded-md border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive"
    >
      <AlertTriangleIcon size={18} className="mt-0.5 shrink-0" />
      <div>
        <div className="font-semibold">Your credit balance is exhausted.</div>
        <div className="mt-1 text-destructive/80">
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
        </div>
      </div>
    </div>
  );
}
