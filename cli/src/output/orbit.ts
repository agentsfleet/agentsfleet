import { PET_NAME } from "../constants/pet.ts";

export type Orbit = {
  ballX: number;
  ballY: number;
  horizontal: -1 | 1;
  vertical: -1 | 1;
  paddle: number;
  score: number;
  missed: boolean;
};

const WIDTH = 25;
const HEIGHT = 8;
const PADDLE = "=====";
const BORDER = `+${"-".repeat(WIDTH)}+`;
const CONTROLS = "Arrows/A/D move  R restart  Q quit";
export const ORBIT_COLUMNS = CONTROLS.length;
export const ORBIT_ROWS = HEIGHT + 5;

export function startOrbit(): Orbit {
  return {ballX: Math.floor(WIDTH / 2), ballY: 1, horizontal: 1, vertical: 1,
    paddle: Math.floor((WIDTH - PADDLE.length) / 2), score: 0, missed: false};
}

export function moveOrbit(state: Orbit, distance: -1 | 1): Orbit {
  const paddle = Math.max(0, Math.min(WIDTH - PADDLE.length, state.paddle + distance));
  return {...state, paddle};
}

export function stepOrbit(state: Orbit): Orbit {
  if (state.missed) return state;
  const horizontal = state.ballX === 0 ? 1 : state.ballX === WIDTH - 1 ? -1 : state.horizontal;
  const ballX = state.ballX + horizontal;
  const ballY = state.ballY + state.vertical;
  if (ballY === HEIGHT - 1) {
    if (ballX >= state.paddle && ballX < state.paddle + PADDLE.length) {
      return {...state, ballX, horizontal, vertical: -1, score: state.score + 1};
    }
    return {...state, ballX, ballY, horizontal, missed: true};
  }
  return {...state, ballX, ballY, horizontal, vertical: ballY === 0 ? 1 : state.vertical};
}

export function orbitFrame(state: Orbit, started: boolean): readonly string[] {
  const rows = Array.from({length: HEIGHT}, (_, y) => {
    const row = y === HEIGHT - 1
      ? " ".repeat(state.paddle) + PADDLE + " ".repeat(WIDTH - state.paddle - PADDLE.length)
      : " ".repeat(WIDTH);
    const content = y === state.ballY ? row.slice(0, state.ballX) + "o" + row.slice(state.ballX + 1) : row;
    return `|${content}|`;
  });
  const hint = !started ? "Press Space to serve." : state.missed ? "Missed! Press R to play again." : "Keep the ball in play.";
  return [`${PET_NAME}  |  Score ${state.score}`, BORDER, ...rows, BORDER, CONTROLS, hint];
}
