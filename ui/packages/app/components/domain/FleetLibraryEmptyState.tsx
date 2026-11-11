import Link from "next/link";
import { Button, EmptyState } from "@agentsfleet/design-system";
import { PuzzlePieceIcon, PlusIcon } from "@phosphor-icons/react/ssr";

export function FleetLibraryEmptyState({createHref, description}: {createHref: string; description: string}) {
  return <EmptyState icon={<PuzzlePieceIcon size={28} aria-hidden="true" />} title="Build your next fleet" description={description}
    action={<Button asChild variant="secondary"><Link href={createHref}><PlusIcon size={16} aria-hidden="true" />New fleet</Link></Button>} />;
}
