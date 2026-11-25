import {afterEach, expect, mock, spyOn, test} from "bun:test";
import {orbitAudio} from "../src/commands/pet/audio.ts";
import {terminal} from "../src/output/palette.ts";

type Child = ReturnType<typeof Bun.spawn>;

function playback() {
  const done = Promise.withResolvers<number>();
  const kill = mock((_signal: string) => {});
  const child = {exited: done.promise, exitCode: null, kill} as unknown as Child;
  return {child, done, kill};
}

afterEach(() => mock.restore());

test("plays bundled clips through the system player and stops the previous clip", async () => {
  const first = playback();
  const second = playback();
  const write = mock((_chunk: string) => {});
  const lookup = spyOn(Bun, "which").mockReturnValue("/usr/bin/afplay");
  const spawn = spyOn(Bun, "spawn").mockReturnValueOnce(first.child).mockReturnValueOnce(second.child);
  const audio = orbitAudio({write}, "darwin");
  audio.play("star");
  expect(lookup).toHaveBeenCalledWith("afplay");
  expect(spawn.mock.calls[0]?.[0]).toEqual(["/usr/bin/afplay", expect.stringContaining("/assets/orbit/star.wav")]);
  audio.play("bounce");
  expect(first.kill).toHaveBeenCalledWith("SIGKILL");
  first.done.resolve(1);
  second.done.resolve(0);
  await second.done.promise;
  expect(write).not.toHaveBeenCalled();
});

test("rings the terminal bell if player startup or playback fails", async () => {
  const write = mock((_chunk: string) => {});
  const failed = playback();
  spyOn(Bun, "which").mockImplementation(name => name === "paplay" ? "/usr/bin/paplay" : null);
  const spawn = spyOn(Bun, "spawn").mockReturnValueOnce(failed.child).mockImplementation(() => {
    throw new Error("Audio is unavailable");
  });
  const audio = orbitAudio({write}, "linux");
  audio.play("miss");
  expect(spawn.mock.calls[0]?.[0]).toEqual(["/usr/bin/paplay", expect.stringContaining("/assets/orbit/miss.wav")]);
  failed.done.resolve(1);
  await failed.done.promise;
  audio.play("bounce");
  expect(write.mock.calls).toEqual([[terminal.bell], [terminal.bell]]);
});

test("keeps playing if a terminal cannot write its bell", () => {
  spyOn(Bun, "which").mockReturnValue(null);
  const audio = orbitAudio({write() {throw new Error("Bell unavailable");}}, "linux");
  expect(() => audio.play("bounce")).not.toThrow();
});
