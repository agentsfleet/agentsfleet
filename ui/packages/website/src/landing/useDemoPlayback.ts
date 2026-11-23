import { useEffect, useRef, useState, type RefObject } from "react";
import { useMotionEnvironment } from "../components/use-motion-environment";

export const DEMO_STAGES = [
  { name: "Slack ticket", duration: 5200 },
  { name: "Grafana investigation", duration: 11000 },
  { name: "Draft fix", duration: 6500 },
  { name: "Verified result", duration: 6800 },
] as const;

const LAST_STAGE = DEMO_STAGES.length - 1;

export function useDemoPlayback(ref: RefObject<HTMLDivElement | null>) {
  const { active, staticMode } = useMotionEnvironment(ref);
  const [stage, setStage] = useState(0);
  const [paused, setPaused] = useState(false);
  const [finished, setFinished] = useState(false);
  const elapsed = useRef(0);
  const playing = active && !paused && !finished;

  useEffect(() => {
    if (!playing) return;
    const duration = DEMO_STAGES[stage].duration;
    const started = performance.now();
    let advanced = false;
    const timer = window.setTimeout(() => {
      advanced = true;
      elapsed.current = 0;
      if (stage === LAST_STAGE) setFinished(true);
      else setStage(stage + 1);
    }, Math.max(0, duration - elapsed.current));

    return () => {
      window.clearTimeout(timer);
      if (!advanced) elapsed.current += performance.now() - started;
    };
  }, [playing, stage]);

  function toggle() {
    if (finished) {
      setFinished(false);
      setStage(0);
      setPaused(false);
    } else {
      setPaused((current) => !current);
    }
  }

  return { stage, paused, finished, playing, staticMode, toggle, pause: () => setPaused(true) };
}
