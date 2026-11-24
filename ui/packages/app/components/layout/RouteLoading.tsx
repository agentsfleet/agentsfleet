import { Spinner } from "@agentsfleet/design-system";

export function RouteLoading({ fullScreen = false, label }: { fullScreen?: boolean; label?: string }) {
  return (
    <div
      className={`flex w-full flex-1 items-center justify-center p-xl ${fullScreen ? "min-h-dvh" : "min-h-full"}`}
    >
      <Spinner variant={fullScreen ? "full-screen" : "page"} label={label ?? "Loading…"} aria-label={label ?? "Loading page"} />
    </div>
  );
}
