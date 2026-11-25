import { fileURLToPath } from "node:url";
import type { WritableStreamLike } from "../../output/capability.ts";
import { terminal } from "../../output/palette.ts";

const CLIPS = {bounce: "bounce.wav", star: "star.wav", miss: "miss.wav"} as const;
const ASSETS = new URL("../../../assets/orbit/", import.meta.url);
const LINUX_PLAYERS = ["pw-play", "paplay", "aplay"] as const;
export type OrbitSound = keyof typeof CLIPS;

type Playback = Pick<Bun.Subprocess, "exitCode" | "exited" | "kill">;

/** One short clip at a time. The bell covers terminals without an audio player. */
export function orbitAudio(screen: WritableStreamLike, platform = process.platform) {
  const players = platform === "darwin" ? ["afplay"] : LINUX_PLAYERS;
  const command = players.map(name => Bun.which(name)).find(Boolean);
  let active: Playback | undefined;
  const bell = () => {
    try {screen.write(terminal.bell);} catch { /* Sound is optional. */ }
  };
  const stop = () => {
    const playing = active;
    active = undefined;
    if (playing?.exitCode === null) playing.kill("SIGKILL");
  };
  const settled = async (child: Playback) => {
    const code = await child.exited;
    if (active !== child) return;
    active = undefined;
    if (code !== 0) bell();
  };
  return {
    stop,
    play(effect: OrbitSound) {
      stop();
      if (!command) {bell(); return;}
      try {
        const file = fileURLToPath(new URL(CLIPS[effect], ASSETS));
        const child = Bun.spawn([command, file], {stdin: "ignore", stdout: "ignore", stderr: "ignore"});
        active = child;
        void settled(child);
      } catch {bell();}
    },
  };
}
