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
const LEVELS = [
  {tickMs: 145, accent: palette.pulse},
  {tickMs: 120, accent: palette.success},
  {tickMs: 95, accent: palette.evidence},
  {tickMs: 75, accent: palette.warn},
] as const;
const STARS = [{x: 20, y: 1}, {x: 8, y: 3}, {x: 25, y: 2}, {x: 15, y: 4}] as const;
const BORDER = `+${"-".repeat(WIDTH)}+`;
const CONTROLS = "Arrows/A/D move  R restart  Q quit";
const READY_HINT = "Press Space to serve.";
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
  return Math.min(LEVELS.length - 1, Math.floor(state.returns / RETURNS_PER_LEVEL));
}

export function orbitTickMs(state: Orbit): number {
  return (LEVELS[levelIndex(state)] ?? LEVELS[0]).tickMs;
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
  if (state.phase === "over") return palette.error("Game over. Press R to restart.", style);
  if (state.phase === "ready") return palette.evidence(READY_HINT, style);
  return palette.success(`Rally x${Math.max(1, state.streak)}   Star * +${STAR_POINTS}`, style);
}

export function orbitFrame(state: Orbit, style: StyleOpts = PLAIN): readonly string[] {
  const stage = levelIndex(state);
  const {accent} = LEVELS[stage] ?? LEVELS[0];
  const ball = state.phase === "ready" ? {x: state.paddle + PADDLE_CENTER, y: COURT_BOTTOM - 1}
    : {x: state.ballX, y: state.ballY};
  const star = STARS[state.target] ?? STARS[0];
  const lifeBar = "o".repeat(state.lives) + ".".repeat(STARTING_LIVES - state.lives);
  const status = `== LEVEL ${stage + 1} == LIVES [${lifeBar}] `.padEnd(WIDTH, "=");
  const boundary = accent("|", style);
  const rows = Array.from({length: HEIGHT}, (_, y) => {
    const cells = Array.from({length: WIDTH}, (_, x) => {
      if (x === ball.x && y === ball.y) return palette.pulse("o", style);
      if (x === star.x && y === star.y) return palette.evidence("*", style);
      const paddleCell = y === COURT_BOTTOM ? PADDLE[x - state.paddle] : undefined;
      if (paddleCell) return palette.pulse(paddleCell, style);
      return (x * 11 + y * 13) % 23 === 3 ? palette.subtle(".", style) : " ";
    });
    return `${boundary}${cells.join("")}${boundary}`;
  });
  const scores = `${palette.pulse(PET_NAME, style)}  Score ${palette.evidence(scoreText(state.score), style)} Best ${palette.success(scoreText(state.best), style)}`;
  return [scores, accent(`+${status}+`, style), ...rows, accent(BORDER, style), CONTROLS, orbitHint(state, style)];
}
