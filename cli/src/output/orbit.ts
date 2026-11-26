import { PET_NAME } from "../constants/pet.ts";
import { ColorMode } from "./capability.ts";
import { palette, type StyleOpts } from "./palette.ts";

type Direction = -1 | 1;
type Horizontal = -1 | -0.5 | 0 | 0.5 | 1;
type Phase = "ready" | "playing" | "over";
export type Orbit = {
  ballX: number;
  ballY: number;
  horizontal: Horizontal;
  vertical: Direction;
  paddle: number;
  phase: Phase;
  lives: number;
  returns: number;
  streak: number;
  score: number;
  best: number;
  target: number;
};

const WIDTH = 31;
const HEIGHT = 8;
const PADDLE = "[===]";
const PADDLE_CENTER = Math.floor(PADDLE.length / 2);
const PADDLE_ROW = HEIGHT - 2; // The last row lets a missed ball fall below the paddle.
const SERVE_ROW = PADDLE_ROW - 1;
const CENTER_ROW = Math.floor((HEIGHT - 1) / 2);
const CENTER_COLUMN = Math.floor(WIDTH / 2);
const PADDLE_AIM: readonly Horizontal[] = [-1, -0.5, 0, 0.5, 1];
const REFLECTED_HORIZONTAL: Record<Horizontal, Horizontal> = {[-1]: 1, [-0.5]: 0.5, 0: 0, [0.5]: -0.5, 1: -1};
const STARTING_LIVES = 3;
const POINTS_PER_RETURN = 10;
const STAR_POINTS = 50;
const RETURNS_PER_LEVEL = 4;
const LEVEL_TICK_MS = [135, 115, 95, 80] as const;
const STARS = [{x: 19, y: 1}, {x: 8, y: 3}, {x: 27, y: 2}, {x: 15, y: 4}] as const;
const BORDER = `+${"-".repeat(WIDTH)}+`;
const LIVES_LABEL = "+= Lives ";
const CONTROLS = "Arrows/A/D move  R restart  Q quit";
const READY_HINT = "Space serve";
const PLAIN: StyleOpts = {mode: ColorMode.NONE};
const COMPACT_SCORE = new Intl.NumberFormat("en-US", {notation: "compact", maximumFractionDigits: 0});
export const ORBIT_COLUMNS = Math.max(WIDTH + 2, CONTROLS.length);
export const ORBIT_ROWS = HEIGHT + 5;

export function startOrbit(best = 0): Orbit {
  const paddle = Math.floor((WIDTH - PADDLE.length) / 2);
  return {ballX: paddle + PADDLE_CENTER, ballY: SERVE_ROW, horizontal: 1, vertical: -1,
    paddle, phase: "ready", lives: STARTING_LIVES, returns: 0, streak: 0, score: 0, best, target: 0};
}

export function serveOrbit(state: Orbit): Orbit {
  if (state.phase !== "ready") return state;
  return {...state, phase: "playing", horizontal: 1, vertical: -1};
}

export function moveOrbit(state: Orbit, distance: Direction): Orbit {
  const paddle = Math.max(0, Math.min(WIDTH - PADDLE.length, state.paddle + distance));
  const ballX = state.phase === "ready" && state.lives === STARTING_LIVES ? paddle + PADDLE_CENTER : state.ballX;
  return {...state, paddle, ballX};
}

function levelIndex(state: Orbit): number {
  return Math.min(LEVEL_TICK_MS.length - 1, Math.floor(state.returns / RETURNS_PER_LEVEL));
}

export function orbitTickMs(state: Orbit): number {
  return LEVEL_TICK_MS[levelIndex(state)] ?? LEVEL_TICK_MS[0];
}

function points(state: Orbit, value: number): Orbit {
  const score = state.score + value;
  return {...state, score, best: Math.max(state.best, score)};
}

function paddleAim(offset: number): Horizontal | undefined {
  // Rounding includes the left half-cell edge. The right edge belongs to the last cell too.
  return offset === PADDLE.length - 0.5 ? 1 : PADDLE_AIM[Math.round(offset)];
}

function landOnPaddle(state: Orbit, contactX: number): Orbit {
  // A path that enters from the side after crossing the top can still touch the paddle.
  const horizontal = paddleAim(contactX - state.paddle) ?? paddleAim(state.ballX - state.paddle);
  if (horizontal === undefined) return state;
  const streak = state.streak + 1;
  return points({...state, ballY: SERVE_ROW, horizontal, vertical: -1, returns: state.returns + 1, streak},
    POINTS_PER_RETURN * streak);
}

function reflectX(proposedX: number): number {
  const boundedX = Math.max(0, Math.min(WIDTH - 1, proposedX));
  return boundedX + (boundedX - proposedX);
}

export function stepOrbit(state: Orbit): Orbit {
  if (state.phase !== "playing") return state;
  const proposedX = state.ballX + state.horizontal;
  const ballX = reflectX(proposedX);
  const horizontal = proposedX === ballX ? state.horizontal : REFLECTED_HORIZONTAL[state.horizontal];
  const ballY = state.ballY + state.vertical;
  if (ballY === HEIGHT) {
    const lives = state.lives - 1;
    return {...state, ballX: CENTER_COLUMN, ballY: CENTER_ROW, lives, streak: 0, phase: lives ? "ready" : "over"};
  }
  const next = {...state, ballX, ballY, horizontal, vertical: ballY === 0 ? 1 as const : state.vertical};
  // The top of the paddle is half a cell above its row center.
  if (state.vertical === 1 && ballY === PADDLE_ROW) return landOnPaddle(next, reflectX(state.ballX + state.horizontal / 2));
  const star = STARS[state.target] ?? STARS[0];
  if (Math.round(ballX) !== star.x || ballY !== star.y) return next;
  return points({...next, target: (state.target + 1) % STARS.length}, STAR_POINTS);
}

function scoreText(score: number): string {
  return score < 100_000 ? String(score).padStart(5, "0") : COMPACT_SCORE.format(score);
}

function orbitHint(state: Orbit, style: StyleOpts): string {
  if (state.phase === "over") return palette.error("Game over", style);
  if (state.phase === "ready") return state.lives === STARTING_LIVES ? READY_HINT : `Missed! ${READY_HINT}`;
  return palette.success(`Rally x${COMPACT_SCORE.format(Math.max(1, state.streak))}`, style);
}

export function orbitFrame(state: Orbit, style: StyleOpts = PLAIN, sound = false): readonly string[] {
  const ballX = Math.round(state.ballX);
  const star = STARS[state.target] ?? STARS[0];
  const lives = `[${"o".repeat(state.lives)}${".".repeat(STARTING_LIVES - state.lives)}]`;
  const top = palette.muted(LIVES_LABEL, style) + palette.error(lives, style)
    + palette.muted(` ${"=".repeat(WIDTH - LIVES_LABEL.length - lives.length)}+`, style);
  const wall = palette.muted("|", style);
  const rows = Array.from({length: HEIGHT}, (_, y) => {
    const cells = Array.from({length: WIDTH}, (_, x) => {
      if (x === ballX && y === state.ballY) return palette.pulse("o", style);
      if (x === star.x && y === star.y) return palette.evidence("*", style);
      const paddleCell = y === PADDLE_ROW ? PADDLE[x - state.paddle] : undefined;
      return paddleCell ? palette.accent(paddleCell, style) : " ";
    });
    return `${wall}${cells.join("")}${wall}`;
  });
  const scores = `${palette.pulse(PET_NAME, style)}  Score ${palette.evidence(scoreText(state.score), style)} Best ${palette.success(scoreText(state.best), style)}`;
  const hint = `${orbitHint(state, style)}  ${palette.accent(sound ? "S sound on" : "S sound off", style)}`;
  return [scores, top, ...rows, palette.muted(BORDER, style), CONTROLS, hint];
}
