import { Skeleton } from "@agentsfleet/design-system";
import { FLEET_VIEW, type FleetView } from "./FleetSubnavigation";

// What a fleet view's panel shows while its data is on the way. The header and
// tabs have already painted by then (the page streams the panel behind a
// boundary), so each shape stands in for that view's own frame and nothing on
// the page jumps when the data lands.

const VIEW_NAME: Record<FleetView, string> = {
  [FLEET_VIEW.chat]: "chat",
  [FLEET_VIEW.events]: "events",
  [FLEET_VIEW.memory]: "memory",
  [FLEET_VIEW.skill]: "skill",
  [FLEET_VIEW.trigger]: "trigger",
};

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
      <output className="sr-only">Loading {VIEW_NAME[view]}</output>
      <Shape />
    </div>
  );
}
