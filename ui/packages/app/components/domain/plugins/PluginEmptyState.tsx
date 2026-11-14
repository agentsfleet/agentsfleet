import Link from "next/link";
import { Button, EmptyState } from "@agentsfleet/design-system";
import { PuzzlePieceIcon, PlusIcon } from "@phosphor-icons/react/ssr";

export function PluginEmptyState({createHref, description, paged}: {createHref: string; description: string; paged: boolean}) {
  return <EmptyState icon={<PuzzlePieceIcon size={24} aria-hidden="true" />} title={paged ? "No plugins on this page" : "No plugins yet"} description={description}
    className="rounded-panel border-solid bg-card py-3xl"
    action={paged ? undefined : <Button asChild><Link href={createHref}><PlusIcon size={16} aria-hidden="true" />Create a plugin</Link></Button>} />;
}
