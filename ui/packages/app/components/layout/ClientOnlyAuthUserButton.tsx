"use client";

import { useEffect } from "react";
import { Button, Spinner } from "@agentsfleet/design-system";
import { RefreshCwIcon } from "lucide-react";
import {
  createIntentModuleLoader,
  INTENT_MODULE_STATUS,
  useIntentModule,
} from "@/components/domain/island-dynamic/intent-module-loader";

const AUTH_BUTTON_PLACEHOLDER_CLASS = "inline-block h-8 w-8";
const authUserMenuLoader = createIntentModuleLoader(
  () => import("./AuthUserMenu"),
);

export default function ClientOnlyAuthUserButton() {
  const menu = useIntentModule(authUserMenuLoader);

  useEffect(() => {
    void authUserMenuLoader.preload();
  }, []);

  if (menu.module) {
    const AuthUserMenu = menu.module.default;
    return <AuthUserMenu />;
  }

  const failed = menu.status === INTENT_MODULE_STATUS.error;
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      className={AUTH_BUTTON_PLACEHOLDER_CLASS}
      aria-label={failed ? "Retry account menu" : "Loading account menu"}
      disabled={!failed}
      onClick={failed ? () => void authUserMenuLoader.retry() : undefined}
    >
      {failed ? (
        <RefreshCwIcon size={16} />
      ) : (
        <Spinner size="sm" srLabel="Loading account menu" />
      )}
    </Button>
  );
}
