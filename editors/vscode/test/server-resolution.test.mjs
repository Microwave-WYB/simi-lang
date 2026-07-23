import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { test } from "node:test";

const require = createRequire(import.meta.url);
const { resolveServerCommand } = require("../src/server.js");

test("configured Simi path has priority", () => {
  assert.equal(
    resolveServerCommand(" /opt/simi/bin/simi ", {
      SIMI_PATH: "/environment/simi",
    }),
    "/opt/simi/bin/simi",
  );
});

test("SIMI_PATH is used when configuration is empty", () => {
  assert.equal(
    resolveServerCommand("  ", { SIMI_PATH: " /environment/simi " }),
    "/environment/simi",
  );
});

test("simi on PATH is the final fallback", () => {
  assert.equal(resolveServerCommand(undefined, {}), "simi");
});
