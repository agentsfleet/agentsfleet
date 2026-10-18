import { buildAcceptanceComponent, componentSchema } from "./acceptance-artifacts.ts";
import { WORKTREE_ROOT } from "./build-source.ts";

if (import.meta.main) await runBuildComponent(Bun.argv.slice(2));

export async function runBuildComponent(args: string[], root = WORKTREE_ROOT): Promise<void> {
  const component = componentSchema.parse(args[0]);
  await buildAcceptanceComponent(component, args.slice(1), {root});
}
