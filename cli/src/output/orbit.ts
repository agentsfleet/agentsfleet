import type { OrbitColor, StyleOpts } from "./palette.ts";
import type { PetState } from "../constants/pet.ts";
import { orbitRow, palette } from "./palette.ts";
import { PET_COLUMNS, PET_NAME, PET_ROWS, PET_STATE } from "../constants/pet.ts";

type Pixel = OrbitColor | null;
interface Pose { readonly angle: number; readonly working: boolean; readonly happy: boolean; readonly blink: boolean; readonly cycle: number }

const CYCLE_FRAMES = 40;
const NAME_ROW = 7;
const CAPTION_ROW = 9;
const CAPTIONS = { idle: "Just floating.", working: "On the lookout.", done: "A little victory dance." } as const;
const TENTACLE_LENGTHS = [10, 14, 12, 14, 10] as const;

export function orbitFrame(tick: number, state: PetState, opts: StyleOpts): readonly string[] {
  const cycle = tick % CYCLE_FRAMES;
  const pose = { angle: cycle / CYCLE_FRAMES * Math.PI * 2, working: state === PET_STATE.working,
    happy: state === PET_STATE.done, blink: cycle === 24 || cycle === 25, cycle };
  const canvas = new OrbitCanvas(Math.round(Math.sin(pose.angle) * (pose.happy ? 2 : 1)));
  tentacles(canvas, pose);
  bell(canvas, pose);
  face(canvas, pose);
  sparks(canvas, pose);
  return Array.from({ length: PET_ROWS }, (_, row) => {
    const top = canvas.rows[row * 2] ?? [];
    const bottom = canvas.rows[row * 2 + 1] ?? [];
    const caption = row === NAME_ROW ? palette.pulse(PET_NAME, opts)
      : row === CAPTION_ROW ? palette.muted(CAPTIONS[state], opts) : "";
    return `${orbitRow(top, bottom, opts)}${caption ? `   ${caption}` : ""}`;
  });
}

class OrbitCanvas {
  readonly #rows: Pixel[][] = Array.from({ length: PET_ROWS * 2 }, () => Array<Pixel>(PET_COLUMNS).fill(null));
  readonly #bob: number;
  constructor(bob: number) { this.#bob = bob; }
  get rows(): readonly (readonly Pixel[])[] { return this.#rows; }
  point(x: number, y: number, color: OrbitColor): void {
    const row = this.#rows[Math.round(y) + this.#bob];
    const column = Math.round(x);
    if (row && column >= 0 && column < PET_COLUMNS) row[column] = color;
  }
  rect(x: number, y: number, width: number, height: number, color: OrbitColor): void {
    for (let yy = y; yy < y + height; yy += 1) for (let xx = x; xx < x + width; xx += 1) this.point(xx, yy, color);
  }
  line(x0: number, y0: number, x1: number, y1: number, color: OrbitColor): void {
    const count = Math.max(Math.abs(x1 - x0), Math.abs(y1 - y0));
    for (let i = 0; i <= count; i += 1) {
      const fraction = count ? i / count : 0;
      this.point(x0 + (x1 - x0) * fraction, y0 + (y1 - y0) * fraction, color);
    }
  }
}

function tentacles(canvas: OrbitCanvas, pose: Pose): void {
  for (const [i, length] of TENTACLE_LENGTHS.entries()) for (let y = 0; y < length; y += 1) {
    const sway = Math.sin(pose.angle * (pose.working ? 2 : 1) + y * .35 + i * .7) *
      (pose.working ? 2 : 1.5) * (y / length);
    const x = 10 + i * 3 + sway;
    const color = i % 2 === 0 ? "M" : "T";
    canvas.point(x, 17 + y, color);
    if (y < length - 3) canvas.point(x + 1, 17 + y, color);
  }
}

function bell(canvas: OrbitCanvas, pose: Pose): void {
  const width = 9 + (pose.working ? Math.round(Math.sin(pose.angle * 2)) : 0);
  for (let y = 4; y <= 18; y += 1) for (let x = 5; x <= 28; x += 1) {
    const distance = ((x - 16.5) / width) ** 2 + ((y - 13) / 9) ** 2;
    if (distance <= 1 && y <= 17 + (Math.abs(x - 16.5) < 5 ? 1 : 0)) {
      canvas.point(x, y, y >= 16 ? "L" : distance > .82 ? "B" : y < 8 ? "L" : "M");
    }
  }
  canvas.line(11, 7, 14, 5, "W"); canvas.line(10, 8, 10, 10, "L"); canvas.point(15, 5, "W");
}

function face(canvas: OrbitCanvas, pose: Pose): void {
  for (const x of [12, 20]) {
    if (pose.blink || pose.happy) {
      canvas.line(x, 12, x + 2, 11, "D");
      if (pose.happy) canvas.point(x + 3, 12, "D");
    } else {
      const glance = pose.working && pose.cycle % 20 >= 10 ? 1 : 0;
      canvas.rect(x + glance, 10, 2, 3, "D"); canvas.point(x + glance, 10, "W");
    }
  }
  canvas.rect(9, 13, 2, 1, "P"); canvas.rect(24, 13, 2, 1, "P");
  if (pose.happy) { canvas.point(16, 13, "D"); canvas.point(17, 14, "D"); canvas.point(18, 13, "D"); }
  else { canvas.point(16, 14, "D"); canvas.point(17, 14, "D"); }
}

function sparks(canvas: OrbitCanvas, pose: Pose): void {
  if (pose.happy) {
    spark(canvas, 4, 8 + Math.round(Math.sin(pose.angle) * 2), "G");
    spark(canvas, 29, 5 + Math.round(Math.cos(pose.angle) * 2), "G");
    if (pose.cycle % 16 < 8) spark(canvas, 29, 22, "T");
  } else if (pose.working) {
    spark(canvas, 16 + Math.round(Math.cos(pose.angle) * 13), 13 + Math.round(Math.sin(pose.angle) * 9), "G");
  }
}

function spark(canvas: OrbitCanvas, x: number, y: number, color: OrbitColor): void {
  canvas.point(x, y - 1, color); canvas.point(x - 1, y, color);
  canvas.point(x + 1, y, color); canvas.point(x, y + 1, color);
}
