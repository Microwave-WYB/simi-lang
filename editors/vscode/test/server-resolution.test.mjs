import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { test } from "node:test";

const require = createRequire(import.meta.url);
const { resolveServerCommand } = require("../src/server.js");

test("configured server path has priority", () => {
  assert.equal(
    resolveServerCommand(" /opt/simi/bin/simi-lsp ", {
      SIMI_LSP_PATH: "/environment/simi-lsp",
    }),
    "/opt/simi/bin/simi-lsp",
  );
});

test("SIMI_LSP_PATH is used when configuration is empty", () => {
  assert.equal(
    resolveServerCommand("  ", { SIMI_LSP_PATH: " /environment/simi-lsp " }),
    "/environment/simi-lsp",
  );
});

test("simi-lsp on PATH is the final fallback", () => {
  assert.equal(resolveServerCommand(undefined, {}), "simi-lsp");
});
