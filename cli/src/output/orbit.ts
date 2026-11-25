import { PET_NAME } from "../constants/pet.ts";
import { ColorMode } from "./capability.ts";
import { palette, type StyleOpts } from "./palette.ts";

type Direction = -1 | 1;
type Phase = "ready" | "playing" | "over";
export type Orbit = {
  ballX: number;
  ballY: number;
  horizontal: Direction;
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
const STARTING_LIVES = 3;
const POINTS_PER_RETURN = 10;
const STAR_POINTS = 50;
const RETURNS_PER_LEVEL = 4;
const LEVEL_TICK_MS = [135, 115, 95, 80] as const;
const STARS = [{x: 20, y: 1}, {x: 8, y: 3}, {x: 27, y: 2}, {x: 15, y: 4}] as const;
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
  return {ballX: paddle + PADDLE_CENTER, ballY: COURT_BOTTOM - 1, horizontal: 1, vertical: -1,
    paddle, phase: "ready", lives: STARTING_LIVES, returns: 0, streak: 0, score: 0, best, target: 0};
}

export function serveOrbit(state: Orbit): Orbit {
  if (state.phase !== "ready") return state;
  return {...state, phase: "playing", ballX: state.paddle + PADDLE_CENTER, ballY: COURT_BOTTOM - 1,
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

function landOnPaddle(state: Orbit, ballX: number, incoming: Direction): Orbit {
  const offset = ballX - state.paddle;
  if (offset < 0 || offset >= PADDLE.length) {
    const lives = state.lives - 1;
    return {...state, ballX, ballY: COURT_BOTTOM, lives, streak: 0, phase: lives ? "ready" : "over"};
  }
  const horizontal = offset === PADDLE_CENTER ? incoming : offset < PADDLE_CENTER ? -1 : 1;
  const streak = state.streak + 1;
  return points({...state, ballX, horizontal, vertical: -1, returns: state.returns + 1, streak},
    POINTS_PER_RETURN * streak);
}

export function stepOrbit(state: Orbit): Orbit {
  if (state.phase !== "playing") return state;
  const horizontal = state.ballX === 0 ? 1 : state.ballX === WIDTH - 1 ? -1 : state.horizontal;
  const ballX = state.ballX + horizontal;
  const ballY = state.ballY + state.vertical;
  if (ballY === COURT_BOTTOM) return landOnPaddle(state, ballX, horizontal);
  const next = {...state, ballX, ballY, horizontal, vertical: ballY === 0 ? 1 as const : state.vertical};
  const star = STARS[state.target] ?? STARS[0];
  if (ballX !== star.x || ballY !== star.y) return next;
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
  const ball = state.phase === "ready" ? {x: state.paddle + PADDLE_CENTER, y: COURT_BOTTOM - 1}
    : {x: state.ballX, y: state.ballY};
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
