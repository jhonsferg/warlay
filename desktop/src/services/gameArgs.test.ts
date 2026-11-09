import assert from "node:assert/strict";
import { test } from "node:test";
import { isGameArgOn, parseGameArgs, setGameArg, withoutExtraArgs } from "./gameArgs.ts";

test("reads known options and keeps the unknown ones", () => {
  const { enabled, extra } = parseGameArgs('-window -nowfpause -loadfile "C:\\maps\\a b.w3x"');
  assert.deepEqual([...enabled].sort(), ["nowfpause", "window"]);
  assert.deepEqual(extra, ["-loadfile", '"C:\\maps\\a b.w3x"']);
});

test("matches case-insensitively and reads two-token options", () => {
  assert.ok(isGameArgOn("-WINDOW", "window"));
  assert.ok(isGameArgOn("-graphicsapi opengl2", "opengl2"));
  assert.ok(!isGameArgOn("-graphicsapi Direct3D11", "opengl2"));
});

test("turning on an option turns off the ones it excludes", () => {
  assert.equal(setGameArg("-window", "nativefullscr", true), "-nativefullscr");
  assert.equal(setGameArg("-opengl", "opengl2", true), "-graphicsapi OpenGL2");
});

test("toggling keeps extras and a stable order", () => {
  assert.equal(setGameArg("-foo -nowfpause", "window", true), "-window -nowfpause -foo");
  assert.equal(setGameArg("-window -foo", "window", false), "-foo");
  assert.equal(setGameArg("", "window", false), "");
});

test("unknown ids change nothing; extras can be dropped", () => {
  assert.equal(setGameArg("-window", "nope", true), "-window");
  assert.equal(withoutExtraArgs("-window -foo bar"), "-window");
});
