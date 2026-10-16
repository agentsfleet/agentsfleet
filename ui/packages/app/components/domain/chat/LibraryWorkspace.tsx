"use client";

import { useRouter } from "next/navigation";
import { workspacePath } from "@/lib/workspace-routes";
import { LibraryBrowser } from "./LibraryBrowser";

export function LibraryWorkspace({ workspace }: { workspace: string }) {
  const router = useRouter();
  return <LibraryBrowser workspace={workspace} onSelect={(source) => {
    const query = new URLSearchParams({ library: source.library_id, revision: source.revision_id });
    router.push(`${workspacePath(workspace, "fleets/new")}?${query}`);
  }} />;
}
