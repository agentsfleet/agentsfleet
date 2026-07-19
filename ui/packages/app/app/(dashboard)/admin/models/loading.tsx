import RouteLoading from "@/components/layout/RouteLoading";

// Without this, /admin/models borrows the dashboard-wide title-less spinner,
// which paints at the top with no header. Match the real Model library header so
// the title doesn't wobble and the spinner reads "<verb> Model library…".
export default function ModelLibraryLoading() {
  return (
    <RouteLoading
      title="Model library"
      description="Every model your team can run, priced per token — the platform default runs for users without their own key."
    />
  );
}
