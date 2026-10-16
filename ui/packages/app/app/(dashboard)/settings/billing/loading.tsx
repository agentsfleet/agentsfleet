import RouteLoading from "@/components/layout/RouteLoading";

// Without this, /settings/billing borrowed the parent settings loader and
// painted the wrong "Workspace" title on navigation. Match the real Billing
// header instantly while the tenant billing + charges reads resolve.
export default function BillingLoading() {
  return (
    <RouteLoading
      title="Billing"
      description="Your credit balance and a clear record of what your fleets use."
    />
  );
}
