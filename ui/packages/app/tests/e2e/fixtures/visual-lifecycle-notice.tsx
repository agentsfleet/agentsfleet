import {useEffect} from "react";
import {useFleetNotice} from "@/components/layout/FleetNoticeProvider";
import {WORKSPACE, FLEET_ID} from "@/tests/helpers/draft-fixtures";

/** Sample-state chrome only; authenticated journeys separately prove the mutation. */
export function VisualLifecycleNotice({outcome}: {outcome: "archived" | "deleted"}) {
  const show = useFleetNotice();
  useEffect(() => {show({outcome, historyHref: `/w/${WORKSPACE}/fleets/${FLEET_ID}`});}, [outcome, show]);
  return null;
}
