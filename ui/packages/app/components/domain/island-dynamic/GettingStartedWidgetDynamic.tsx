"use client";

import { useEffect, type ComponentProps } from "react";
import { Button } from "@agentsfleet/design-system";
import { RefreshCwIcon } from "lucide-react";
import type GettingStartedWidget from "@/components/layout/GettingStartedWidget";
import {
  createIntentModuleLoader,
  INTENT_MODULE_STATUS,
  useIntentModule,
} from "./intent-module-loader";

const gettingStartedWidgetLoader = createIntentModuleLoader(
  () => import("@/components/layout/GettingStartedWidget"),
);

export default function GettingStartedWidgetDynamic(
  props: ComponentProps<typeof GettingStartedWidget>,
) {
  const widget = useIntentModule(gettingStartedWidgetLoader);

  useEffect(() => {
    void gettingStartedWidgetLoader.preload();
  }, []);

  if (widget.module) {
    const LoadedGettingStartedWidget = widget.module.default;
    return <LoadedGettingStartedWidget {...props} />;
  }
  if (widget.status !== INTENT_MODULE_STATUS.error) return null;

  return (
    <Button
      type="button"
      variant="secondary"
      size="sm"
      onClick={() => void gettingStartedWidgetLoader.retry()}
    >
      <RefreshCwIcon size={14} />
      Retry getting started
    </Button>
  );
}
