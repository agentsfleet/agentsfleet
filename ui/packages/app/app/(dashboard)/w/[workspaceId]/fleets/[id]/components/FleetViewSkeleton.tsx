import { Skeleton } from "@agentsfleet/design-system";
import { FLEET_VIEW, type FleetView } from "./FleetSubnavigation";

// What a fleet view's panel shows while its data is on the way. The header and
// tabs have already painted by then (the page streams the panel behind a
// boundary), so each shape stands in for that view's own frame and nothing on
// the page jumps when the data lands.

const LIST_ROWS = 6;

function ListShape() {
  return (
    <div className="flex flex-col gap-sm">
      {Array.from({ length: LIST_ROWS }, (_, index) => (
        <Skeleton key={index} className="h-12 w-full rounded-md" />
      ))}
    </div>
  );
}

function ChatShape() {
  return (
    <div className="flex min-h-0 flex-1 flex-col justify-end gap-md">
      <Skeleton className="h-16 w-2/3 rounded-lg" />
      <Skeleton className="ml-auto h-12 w-1/2 rounded-lg" />
      <Skeleton className="h-24 w-2/3 rounded-lg" />
      <Skeleton className="h-12 w-full rounded-lg" />
    </div>
  );
}

function EditorShape() {
  return <Skeleton className="h-96 w-full rounded-lg" />;
}

const SHAPE: Record<FleetView, () => React.JSX.Element> = {
  [FLEET_VIEW.chat]: ChatShape,
  [FLEET_VIEW.events]: ListShape,
  [FLEET_VIEW.memory]: ListShape,
  [FLEET_VIEW.skill]: EditorShape,
  [FLEET_VIEW.trigger]: EditorShape,
};

export function FleetViewSkeleton({ view }: { view: FleetView }) {
  const Shape = SHAPE[view];
  return (
    <div aria-busy="true" className="flex min-h-0 flex-1 flex-col">
      {/* A view's value is its name (`FLEET_VIEW`), so it reads as itself. */}
      <output className="sr-only">Loading {view}</output>
      <Shape />
    </div>
  );
}
