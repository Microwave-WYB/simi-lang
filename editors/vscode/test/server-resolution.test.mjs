import assert from "node:assert/strict";
import { createRequire } from "node:module";
import path from "node:path";
import { test } from "node:test";

const require = createRequire(import.meta.url);
const { resolveServerCommand } = require("../src/server.js");

test("configured Simi path has priority", () => {
  assert.equal(
    resolveServerCommand(
      " /opt/simi/bin/simi ",
      { SIMI_PATH: "/environment/simi" },
      "/extension",
      "linux",
      () => true,
    ),
    "/opt/simi/bin/simi",
  );
});

test("SIMI_PATH is used when configuration is empty", () => {
  assert.equal(
    resolveServerCommand(
      "  ",
      { SIMI_PATH: " /environment/simi " },
      "/extension",
      "linux",
      () => true,
    ),
    "/environment/simi",
  );
});

test("a bundled server is preferred over PATH", () => {
  const extensionPath = path.join("", "extension");
  const expected = path.join(extensionPath, "bin", "simi");
  assert.equal(
    resolveServerCommand(undefined, {}, extensionPath, "linux", (candidate) => candidate === expected),
    expected,
  );
});

test("the bundled Windows server uses the executable suffix", () => {
  const extensionPath = path.join("", "extension");
  const expected = path.join(extensionPath, "bin", "simi.exe");
  assert.equal(
    resolveServerCommand(undefined, {}, extensionPath, "win32", (candidate) => candidate === expected),
    expected,
  );
});

test("simi on PATH is the final fallback", () => {
  assert.equal(resolveServerCommand(undefined, {}, "/extension", "linux", () => false), "simi");
});
