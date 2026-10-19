"use client";

import { useEffect } from "react";
import { Button } from "@agentsfleet/design-system";
import { createIntentModuleLoader, useIntentModule } from "@/components/domain/island-dynamic/intent-module-loader";
import type { DraftSearchResultsProps } from "./DraftSearchResults";
const loader = createIntentModuleLoader(() => import("./DraftSearchResults"));

export function DraftSearchResultsDynamic(props: DraftSearchResultsProps) {
  const search = useIntentModule(loader);
  useEffect(() => {void loader.preload();}, []);
  if (search.module) return <search.module.DraftSearchResults {...props} />;
  return <output className="block px-md py-sm text-label text-muted-foreground">
    {search.status === "error" ? <>Couldn’t load chat search. <Button variant="ghost" size="sm" onClick={() => {void loader.retry();}}>Retry search</Button></> : "Searching chats…"}
  </output>;
}
