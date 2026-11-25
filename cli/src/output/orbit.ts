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
const COURT_BOTTOM = HEIGHT - 1;
const CONTACT_ROW = COURT_BOTTOM - 1;
const SERVE_ROW = CONTACT_ROW - 1;
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
  return {...state, phase: "playing", ballX: state.paddle + PADDLE_CENTER, ballY: SERVE_ROW,
    horizontal: 1, vertical: -1};
}

export function moveOrbit(state: Orbit, distance: Direction): Orbit {
  const paddle = Math.max(0, Math.min(WIDTH - PADDLE.length, state.paddle + distance));
  return {...state, paddle};
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

function landOnPaddle(state: Orbit, ballX: number, column: number): Orbit {
  const horizontal = PADDLE_AIM[column - state.paddle];
  if (horizontal === undefined) {
    const lives = state.lives - 1;
    return {...state, ballX, ballY: CONTACT_ROW, lives, streak: 0, phase: lives ? "ready" : "over"};
  }
  const streak = state.streak + 1;
  return points({...state, ballX, ballY: CONTACT_ROW, horizontal, vertical: -1, returns: state.returns + 1, streak},
    POINTS_PER_RETURN * streak);
}

export function stepOrbit(state: Orbit): Orbit {
  if (state.phase !== "playing") return state;
  const proposedX = state.ballX + state.horizontal;
  const boundedX = Math.max(0, Math.min(WIDTH - 1, proposedX));
  const horizontal = proposedX === boundedX ? state.horizontal : REFLECTED_HORIZONTAL[state.horizontal];
  // Reflect any part of the step that crosses a wall.
  const ballX = boundedX + (boundedX - proposedX);
  const ballY = state.ballY + state.vertical;
  const column = Math.round(ballX);
  // Contact is the last ball row. The next tick travels up immediately.
  if (state.vertical === 1 && ballY === CONTACT_ROW) return landOnPaddle(state, ballX, column);
  const next = {...state, ballX, ballY, horizontal, vertical: ballY === 0 ? 1 as const : state.vertical};
  const star = STARS[state.target] ?? STARS[0];
  if (column !== star.x || ballY !== star.y) return next;
  return points({...next, target: (state.target + 1) % STARS.length}, STAR_POINTS);
}

function scoreText(score: number): string {
  return score < 100_000 ? String(score).padStart(5, "0") : COMPACT_SCORE.format(score);
}

function orbitHint(state: Orbit, style: StyleOpts): string {
  if (state.phase === "over") return palette.error("Game over", style);
  if (state.phase === "ready") return READY_HINT;
  return palette.success(`Rally x${COMPACT_SCORE.format(Math.max(1, state.streak))}`, style);
}

export function orbitFrame(state: Orbit, style: StyleOpts = PLAIN, sound = false): readonly string[] {
  const ball = state.phase === "ready" ? {x: state.paddle + PADDLE_CENTER, y: SERVE_ROW}
    : {x: Math.round(state.ballX), y: state.ballY};
  const star = STARS[state.target] ?? STARS[0];
  const lives = `[${"o".repeat(state.lives)}${".".repeat(STARTING_LIVES - state.lives)}]`;
  const top = palette.muted(LIVES_LABEL, style) + palette.error(lives, style)
    + palette.muted(` ${"=".repeat(WIDTH - LIVES_LABEL.length - lives.length)}+`, style);
  const wall = palette.muted("|", style);
  const rows = Array.from({length: HEIGHT}, (_, y) => {
    const cells = Array.from({length: WIDTH}, (_, x) => {
      if (x === ball.x && y === ball.y) return palette.pulse("o", style);
      if (x === star.x && y === star.y) return palette.evidence("*", style);
      const paddleCell = y === COURT_BOTTOM ? PADDLE[x - state.paddle] : undefined;
      return paddleCell ? palette.accent(paddleCell, style) : " ";
    });
    return `${wall}${cells.join("")}${wall}`;
  });
  const scores = `${palette.pulse(PET_NAME, style)}  Score ${palette.evidence(scoreText(state.score), style)} Best ${palette.success(scoreText(state.best), style)}`;
  const hint = `${orbitHint(state, style)}  ${palette.accent(sound ? "S sound on" : "S sound off", style)}`;
  return [scores, top, ...rows, palette.muted(BORDER, style), CONTROLS, hint];
}
