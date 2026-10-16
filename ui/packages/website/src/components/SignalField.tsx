import { useId, useRef } from "react";
import { useMotionEnvironment } from "./use-motion-environment";

const COLUMNS = 96;
const ROWS = 28;
const STEP = 12;
const SIGNAL_DOTS = Array.from({ length: COLUMNS * ROWS }, (_, index) => {
  const x = index % COLUMNS;
  const y = Math.floor(index / COLUMNS);
  const ridge = 12 + Math.sin(x / 13) * 5 + Math.cos(x / 7) * 2;
  const distance = Math.abs(y - ridge);
  const strength = Math.max(0, 1 - distance / 9) * (0.6 + Math.sin(x * 3 + y * 7) * 0.4);
  return strength > 0.18 ? `M${x * STEP},${y * STEP}h${strength > 0.65 ? 2 : 1}v1h-1z` : "";
}).join("");

export function SignalField() {
  const mask = useId();
  const ref = useRef<SVGSVGElement>(null);
  const { active } = useMotionEnvironment(ref);
  return (
    <svg ref={ref} data-running={active} className="signal-field" viewBox="0 0 1152 336" fill="currentColor" aria-hidden="true" focusable="false">
      <defs>
        <linearGradient id={mask}><stop stopColor="white" stopOpacity="0.8" /><stop offset="0.45" stopColor="white" stopOpacity="0.06" /><stop offset="1" stopColor="white" stopOpacity="0.9" /></linearGradient>
        <mask id={`${mask}-field`}><path d="M0 0h1152v336H0z" fill={`url(#${mask})`} /></mask>
      </defs>
      <g mask={`url(#${mask}-field)`}><path d={SIGNAL_DOTS} /></g>
    </svg>
  );
}
