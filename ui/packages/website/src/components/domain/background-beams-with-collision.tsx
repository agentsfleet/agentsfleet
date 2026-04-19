"use client";

import { useEffect, useRef, useState } from "react";
import { LazyMotion, domAnimation, m, AnimatePresence } from "motion/react";
import { usePrefersReducedMotion } from "./use-in-view";

/*
 * BackgroundBeamsWithCollision — website-only atmospheric drama for
 * the /agents hero section (spec §5.8.3). Vertical orange→transparent
 * gradient beams fall from above the viewport; when a beam's bottom
 * edge hits the floor ref, a particle burst renders, the beam holds
 * for ~2s, then resets and re-fires.
 *
 * Decoration only: `role="presentation"` + `aria-hidden="true"`; the
 * component contributes zero nodes to the a11y tree. `pointer-events:
 * none` so clicks pass through. `prefers-reduced-motion: reduce`
 * collapses everything to a static gradient so there is no motion.
 */

type BeamConfig = {
  readonly id: string;
  readonly initialX: number;
  readonly translateX: number;
  readonly duration: number;
  readonly repeatDelay: number;
  readonly delay: number;
  readonly className?: string;
};

const BEAMS: readonly BeamConfig[] = [
  { id: "b1", initialX: 10, translateX: 10, duration: 7, repeatDelay: 3, delay: 0.5 },
  { id: "b2", initialX: 600, translateX: 600, duration: 3, repeatDelay: 3, delay: 1.2 },
  { id: "b3", initialX: 100, translateX: 100, duration: 7, repeatDelay: 7, delay: 2 },
  { id: "b4", initialX: 400, translateX: 400, duration: 5, repeatDelay: 4, delay: 1.5 },
  { id: "b5", initialX: 800, translateX: 800, duration: 11, repeatDelay: 2, delay: 0 },
  { id: "b6", initialX: 1000, translateX: 1000, duration: 4, repeatDelay: 2, delay: 3 },
  { id: "b7", initialX: 1200, translateX: 1200, duration: 6, repeatDelay: 4, delay: 1 },
];

export interface BackgroundBeamsWithCollisionProps {
  children?: React.ReactNode;
  className?: string;
}

export function BackgroundBeamsWithCollision({
  children,
  className,
}: BackgroundBeamsWithCollisionProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const parentRef = useRef<HTMLDivElement>(null);
  const reducedMotion = usePrefersReducedMotion();

  return (
    <LazyMotion features={domAnimation} strict>
      <div
        ref={parentRef}
        className={[
          "relative flex min-h-[20rem] w-full items-center justify-center overflow-hidden",
          "bg-gradient-to-b from-background to-[var(--z-bg-1)]",
          className ?? "",
        ].join(" ")}
        role="presentation"
      >
        {!reducedMotion
          ? BEAMS.map((beam) => (
              <CollisionBeam
                key={beam.id}
                beam={beam}
                containerRef={containerRef}
                parentRef={parentRef}
              />
            ))
          : null}

        {children}

        <div
          ref={containerRef}
          aria-hidden="true"
          className="pointer-events-none absolute inset-x-0 bottom-0 w-full bg-transparent"
          style={{ boxShadow: "0 0 24px rgba(34,42,53,0.64), 0 1px 1px rgba(34,42,53,0.5)" }}
        />
      </div>
    </LazyMotion>
  );
}

type CollisionState = {
  detected: boolean;
  coords: { x: number; y: number } | null;
  key: number;
};

function CollisionBeam({
  beam,
  containerRef,
  parentRef,
}: {
  beam: BeamConfig;
  containerRef: React.RefObject<HTMLDivElement | null>;
  parentRef: React.RefObject<HTMLDivElement | null>;
}) {
  const beamRef = useRef<HTMLDivElement>(null);
  const [collision, setCollision] = useState<CollisionState>({
    detected: false,
    coords: null,
    key: 0,
  });
  const [cycleKey, setCycleKey] = useState(0);

  useEffect(() => {
    if (typeof window === "undefined") return;
    let raf = 0;

    const check = () => {
      const beamEl = beamRef.current;
      const floorEl = containerRef.current;
      const parentEl = parentRef.current;
      if (beamEl && floorEl && parentEl && !collision.detected) {
        const beamRect = beamEl.getBoundingClientRect();
        const floorRect = floorEl.getBoundingClientRect();
        const parentRect = parentEl.getBoundingClientRect();
        if (beamRect.bottom >= floorRect.top) {
          setCollision({
            detected: true,
            coords: {
              x: beamRect.left - parentRect.left + beamRect.width / 2,
              y: beamRect.bottom - parentRect.top,
            },
            key: collision.key + 1,
          });
        }
      }
      raf = window.requestAnimationFrame(check);
    };

    raf = window.requestAnimationFrame(check);
    return () => window.cancelAnimationFrame(raf);
  }, [collision, containerRef, parentRef]);

  useEffect(() => {
    if (!collision.detected || !collision.coords) return;
    const resetTimer = window.setTimeout(() => {
      setCollision({ detected: false, coords: null, key: collision.key });
      setCycleKey((k) => k + 1);
    }, 2000);
    return () => window.clearTimeout(resetTimer);
  }, [collision]);

  return (
    <>
      <m.div
        key={cycleKey}
        ref={beamRef}
        data-testid="beam"
        data-collision-detected={collision.detected ? "true" : "false"}
        initial={{ translateY: "-200px", translateX: `${beam.initialX}px`, rotate: 0 }}
        animate={{ translateY: "1800px", translateX: `${beam.translateX}px`, rotate: 0 }}
        transition={{
          duration: beam.duration,
          repeat: Infinity,
          repeatType: "loop",
          ease: "linear",
          delay: beam.delay,
          repeatDelay: beam.repeatDelay,
        }}
        className={[
          "absolute left-0 top-20 m-auto h-14 w-px rounded-full",
          "bg-gradient-to-t from-primary via-[var(--primary-bright)] to-transparent",
          beam.className ?? "",
        ].join(" ")}
        aria-hidden="true"
      />
      <AnimatePresence>
        {collision.detected && collision.coords ? (
          <Explosion
            key={`explosion-${collision.key}`}
            style={{
              left: `${collision.coords.x}px`,
              top: `${collision.coords.y}px`,
              transform: "translate(-50%, -50%)",
            }}
          />
        ) : null}
      </AnimatePresence>
    </>
  );
}

// Deterministic particle directions. Hand-tuned to feel random while
// keeping the render pure — React 19's concurrent rendering contract
// (and the react-hooks/purity lint) disallows Math.random() on the
// render path.
const EXPLOSION_PARTICLES: ReadonlyArray<{
  id: number;
  directionX: number;
  directionY: number;
  duration: number;
}> = [
  { id: 0, directionX: -32, directionY: -44, duration: 1.4 },
  { id: 1, directionX: 24, directionY: -52, duration: 1.8 },
  { id: 2, directionX: -12, directionY: -38, duration: 0.9 },
  { id: 3, directionX: 36, directionY: -28, duration: 1.2 },
  { id: 4, directionX: -28, directionY: -18, duration: 0.7 },
  { id: 5, directionX: 18, directionY: -48, duration: 1.6 },
  { id: 6, directionX: -40, directionY: -32, duration: 1.1 },
  { id: 7, directionX: 8, directionY: -56, duration: 1.9 },
  { id: 8, directionX: 30, directionY: -14, duration: 0.6 },
  { id: 9, directionX: -22, directionY: -50, duration: 1.3 },
  { id: 10, directionX: 4, directionY: -22, duration: 0.8 },
  { id: 11, directionX: -36, directionY: -40, duration: 1.7 },
  { id: 12, directionX: 26, directionY: -36, duration: 1.0 },
  { id: 13, directionX: -8, directionY: -58, duration: 2.0 },
  { id: 14, directionX: 34, directionY: -46, duration: 1.5 },
  { id: 15, directionX: -16, directionY: -24, duration: 0.75 },
  { id: 16, directionX: 12, directionY: -42, duration: 1.25 },
  { id: 17, directionX: -30, directionY: -20, duration: 0.65 },
  { id: 18, directionX: 22, directionY: -54, duration: 1.85 },
  { id: 19, directionX: -4, directionY: -34, duration: 0.95 },
];

function Explosion({ style }: { style: React.CSSProperties }) {
  return (
    <div
      className="absolute z-50 h-2 w-2"
      style={style}
      aria-hidden="true"
      data-testid="explosion"
    >
      <m.div
        initial={{ opacity: 0.7 }}
        animate={{ opacity: 0 }}
        transition={{ duration: 1.5, ease: "easeOut" }}
        className="absolute -inset-x-10 top-0 m-auto h-2 w-10 rounded-full bg-gradient-to-r from-transparent via-info to-transparent blur-sm"
      />
      {EXPLOSION_PARTICLES.map((p) => (
        <m.span
          key={p.id}
          initial={{ x: 0, y: 0, opacity: 1 }}
          animate={{
            x: p.directionX,
            y: p.directionY,
            opacity: 0,
          }}
          transition={{ duration: p.duration, ease: "easeOut" }}
          className="absolute h-1 w-1 rounded-full bg-gradient-to-b from-info to-primary"
        />
      ))}
    </div>
  );
}

export default BackgroundBeamsWithCollision;
