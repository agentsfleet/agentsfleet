import RouteLoading from "@/components/layout/RouteLoading";

// Without this, /admin/runners borrows the dashboard-wide title-less spinner,
// which paints at the top with no header. Match the real Runners header so the
// title doesn't wobble and the spinner reads "Loading Runners…".
export default function RunnersLoading() {
  return (
    <RouteLoading
      title="Runners"
      description="Hosts you enroll to run fleets."
    />
  );
}
