import { Spinner } from "@agentsfleet/design-system";

export function RouteLoading({ fullScreen = false }: { fullScreen?: boolean }) {
  return (
    <div
      data-page-loading="true"
      className={`flex w-full flex-1 items-center justify-center p-xl ${fullScreen ? "min-h-dvh" : "min-h-full"}`}
    >
      <Spinner size="lg" label="Loading…" aria-label="Loading page" />
    </div>
  );
}
