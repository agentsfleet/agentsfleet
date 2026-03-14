export const MODE_HUMANS = "humans";
export const MODE_AGENTS = "agents";

export type Mode = typeof MODE_HUMANS | typeof MODE_AGENTS;

export const MODE_PATHS: Record<Mode, string> = {
  [MODE_HUMANS]: "/",
  [MODE_AGENTS]: "/agents",
};

export function getModeFromPathname(pathname: string): Mode {
  return pathname === MODE_PATHS[MODE_AGENTS] ? MODE_AGENTS : MODE_HUMANS;
}
