import {expect, test} from "bun:test";
import {stripVTControlCharacters} from "node:util";
import {ColorMode} from "../src/output/capability.ts";
import {moveOrbit, orbitFrame, orbitTickMs, ORBIT_COLUMNS, ORBIT_ROWS, serveOrbit, startOrbit, stepOrbit} from "../src/output/orbit.ts";
import {terminal} from "../src/output/palette.ts";

const ESC = "\u001b[";

test("draws a fixed-size colorful court with a readable plain-text fallback", () => {
  const frame = orbitFrame(startOrbit());
  expect(frame).toHaveLength(ORBIT_ROWS);
  expect(frame[0]).toBe("Orbit  Score 00000 Best 00000");
  expect(frame[1]).toContain("Lives [ooo]");
  expect(frame[2]).toBe("|" + " ".repeat(31) + "|");
  expect(frame[3]).toContain("*");
  expect(frame[7]).toContain("o");
  expect(frame[8]).toContain("[===]");
  expect(frame[9]).toBe("|" + " ".repeat(31) + "|");
  expect(frame.at(-1)).toBe("Space serve  S sound off");
  expect(frame.every(row => row.length <= ORBIT_COLUMNS && /^[ -~]+$/u.test(row))).toBe(true);
  const colored = orbitFrame(startOrbit(), {mode: ColorMode.XTERM256});
  expect(colored[3]).toContain(`${ESC}38;5;220m*`);
  expect(colored[8]).toContain(`${ESC}38;5;183m[`);
  expect(colored.map(stripVTControlCharacters)).toEqual([...frame]);
  expect(orbitFrame(startOrbit(100_000))[0]).toContain("Best 100K");
});

test("aims with the paddle, multiplies the score on a streak and speeds up on the fourth return", () => {
  const playing = serveOrbit(startOrbit());
  const right = stepOrbit({...playing, ballX: 16, vertical: 1});
  expect(right).toMatchObject({ballX: 17, ballY: 5, horizontal: 1, vertical: -1, streak: 1, score: 10});
  const left = stepOrbit({...right, ballX: 13, ballY: 5, vertical: 1, returns: 3});
  expect(left).toMatchObject({ballX: 14, horizontal: -0.5, vertical: -1, streak: 2, score: 30, returns: 4});
  expect(orbitTickMs(left)).toBeLessThan(orbitTickMs(playing));
  expect(orbitFrame(left)[1]).toBe(orbitFrame(playing)[1]);
  expect(orbitFrame(left).at(-1)).toBe("Rally x2  S sound off");
  expect(stepOrbit({...playing, ballX: 30, ballY: 1})).toMatchObject({ballX: 29, ballY: 0, horizontal: -1, vertical: 1});
  expect(stepOrbit({...playing, ballX: 0, horizontal: -1})).toMatchObject({ballX: 1, horizontal: 1});
});

test("a center hit lifts off immediately and a slow shot reflects from the wall", () => {
  const playing = serveOrbit(startOrbit());
  const contact = stepOrbit({...playing, ballX: 14, vertical: 1});
  expect(contact).toMatchObject({ballX: 15, ballY: 5, horizontal: 0, returns: 1});
  expect(stepOrbit(contact)).toMatchObject({ballX: 15, ballY: 4, returns: 1});
  const wall = stepOrbit({...playing, ballX: 0, horizontal: -0.5});
  expect(wall).toMatchObject({ballX: 0.5, horizontal: 0.5});
});

test("a ball that arrives at a visible edge from the side still rebounds", () => {
  const playing = serveOrbit(startOrbit());
  const grazed = stepOrbit({...playing, ballX: 12, horizontal: 0.5, vertical: 1});
  expect(grazed).toMatchObject({ballX: 12.5, returns: 1, lives: 3, horizontal: -1});
  expect(orbitFrame(grazed)[7]?.[14]).toBe("o");
  const missed = stepOrbit({...playing, ballX: 11.5, horizontal: 0.5, vertical: 1});
  expect(missed).toMatchObject({ballY: 6, phase: "playing", lives: 3, returns: 0});
});

test.each([
  {edge: "left", ballX: 13, horizontal: -1, projectedX: 12},
  {edge: "right", ballX: 17, horizontal: 1, projectedX: 18},
] as const)("returns a ball that grazes the $edge paddle edge between frames", ({ballX, horizontal, projectedX}) => {
  const playing = serveOrbit(startOrbit());
  expect(stepOrbit({...playing, ballX, horizontal, vertical: 1})).toMatchObject({
    ballX: projectedX, ballY: 5, horizontal, vertical: -1, score: 10, returns: 1, lives: 3,
  });
});

test("collects the opening star for points and preserves the session best on restart", () => {
  let state = serveOrbit(startOrbit());
  for (let tick = 0; tick < 4; tick += 1) state = stepOrbit(state);
  expect(state).toMatchObject({ballX: 19, ballY: 1, target: 1, score: 50, best: 50});
  expect(startOrbit(state.best)).toMatchObject({best: 50, score: 0, streak: 0, phase: "ready", target: 0});
});

test("keeps a star visually distinct from the ball on an adjacent tile", () => {
  const approach = {...serveOrbit(startOrbit()), target: 2, ballX: 25, ballY: 3};
  const near = stepOrbit(approach);
  expect(near.score).toBe(0);
  expect(orbitFrame(near)[4]).toMatch(/o.*\*/u);
  expect(stepOrbit({...approach, ballX: 26}).score).toBe(50);
});

test("keeps the paddle on court and follows it before the first serve", () => {
  expect(moveOrbit({...startOrbit(), paddle: 0}, -1).paddle).toBe(0);
  expect(moveOrbit({...startOrbit(), paddle: 26}, 1).paddle).toBe(26);
  expect(serveOrbit(moveOrbit(startOrbit(), 1))).toMatchObject({ballX: 16, ballY: 5});
});

test("lets a miss fall below the paddle before using a life and resetting to center", () => {
  const playing = {...serveOrbit(startOrbit()), ballX: 0, horizontal: 0, vertical: 1, streak: 2, score: 50} as const;
  const alongside = stepOrbit(playing);
  expect(alongside).toMatchObject({ballY: 6, lives: 3, phase: "playing"});
  const below = stepOrbit(alongside);
  expect(below).toMatchObject({ballY: 7, lives: 3, phase: "playing"});
  expect(orbitFrame(below)[9]?.[1]).toBe("o");
  const ready = stepOrbit(below);
  expect(ready).toMatchObject({ballX: 15, ballY: 3, phase: "ready", lives: 2, streak: 0, score: 50});
  expect(orbitFrame(ready)[5]?.[16]).toBe("o");
  expect(orbitFrame(ready).at(-1)).toBe("Missed! Space serve  S sound off");
  expect(stepOrbit(ready)).toBe(ready);
  expect(serveOrbit(moveOrbit(ready, -1))).toMatchObject({ballX: 15, ballY: 3, phase: "playing"});
});

test("ends the game on the last miss and holds the centered ball until restart", () => {
  const lose = (state: ReturnType<typeof startOrbit>) => stepOrbit({...serveOrbit(state), ballY: 7, vertical: 1});
  const first = lose({...startOrbit(), streak: 2});
  expect(first).toMatchObject({phase: "ready", lives: 2, streak: 0});
  expect(stepOrbit(first)).toBe(first);
  const state = lose(lose(first));
  expect(state).toMatchObject({ballX: 15, ballY: 3, phase: "over", lives: 0});
  expect(serveOrbit(state)).toBe(state);
  expect(stepOrbit(state)).toBe(state);
  expect(orbitFrame(state).at(-1)).toBe("Game over  S sound off");
});

test("redraw and cleanup stay within their reserved rows", () => {
  expect(terminal.reserveRows(2)).toBe("\r\n\r\n");
  expect(terminal.repaintRows(["one", "two"])).toBe(`${ESC}2A\r${ESC}2Kone\r\n${ESC}2Ktwo\r\n`);
  expect(terminal.clearRows(2)).toBe(`${ESC}2A\r${ESC}2K\r\n${ESC}2K${ESC}1A\r`);
});
