import {expect, test} from "bun:test";
import {moveOrbit, orbitFrame, ORBIT_COLUMNS, ORBIT_ROWS, startOrbit, stepOrbit} from "../src/output/orbit.ts";
import {terminal} from "../src/output/palette.ts";

const ESC = "\u001b[";

test("draws a fixed-size ASCII court with a serve prompt", () => {
  const frame = orbitFrame(startOrbit(), false);
  expect(frame).toHaveLength(ORBIT_ROWS);
  expect(frame[0]).toBe("Orbit  |  Score 0");
  expect(frame[3]).toContain("o");
  expect(frame[9]).toContain("=====");
  expect(frame.at(-1)).toBe("Press Space to serve.");
  expect(frame.every(row => row.length <= ORBIT_COLUMNS && /^[ -~]+$/u.test(row))).toBe(true);
});

test("bounces at the walls and scores when the paddle returns the ball", () => {
  let state = startOrbit();
  for (let move = 0; move < 6; move += 1) state = moveOrbit(state, 1);
  for (let tick = 0; tick < 6; tick += 1) state = stepOrbit(state);
  expect(state).toMatchObject({ballX: 18, ballY: 6, vertical: -1, score: 1});
  expect(orbitFrame(state, true).at(-1)).toBe("Keep the ball in play.");
  for (let tick = 0; tick < 6; tick += 1) state = stepOrbit(state);
  expect(state).toMatchObject({ballX: 24, ballY: 0, vertical: 1, score: 1});
  expect(stepOrbit(state)).toMatchObject({ballX: 23, ballY: 1, horizontal: -1});
  expect(stepOrbit({...state, ballX: 0, horizontal: -1})).toMatchObject({ballX: 1, horizontal: 1});
});

test("keeps the paddle inside the court and holds a missed ball until restart", () => {
  expect(moveOrbit({...startOrbit(), paddle: 0}, -1).paddle).toBe(0);
  expect(moveOrbit({...startOrbit(), paddle: 20}, 1).paddle).toBe(20);
  let state = startOrbit();
  for (let tick = 0; tick < 6; tick += 1) state = stepOrbit(state);
  expect(state).toMatchObject({missed: true, score: 0});
  expect(stepOrbit(state)).toBe(state);
  expect(orbitFrame(state, true).at(-1)).toBe("Missed! Press R to play again.");
});

test("redraw and cleanup stay within their reserved rows", () => {
  expect(terminal.reserveRows(2)).toBe("\r\n\r\n");
  expect(terminal.repaintRows(["one", "two"])).toBe(`${ESC}2A\r${ESC}2Kone\r\n${ESC}2Ktwo\r\n`);
  expect(terminal.clearRows(2)).toBe(`${ESC}2A\r${ESC}2K\r\n${ESC}2K${ESC}1A\r`);
});
