import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { chmodSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";

const extensionPath = fileURLToPath(new URL("../", import.meta.url));
const generatorPath = fileURLToPath(new URL("generate-parser.mjs", import.meta.url));
const parserPath = fileURLToPath(new URL("../syntaxes/tree-sitter-simi.wasm", import.meta.url));
const originalParser = readFileSync(parserPath);

if (process.platform !== "win32") {
  chmodSync(parserPath, 0o755);
}
execFileSync(process.execPath, [generatorPath], { stdio: "inherit" });

assert.deepEqual(readFileSync(parserPath), originalParser, "generated parser bytes must be reproducible");
if (process.platform !== "win32") {
  assert.equal(statSync(parserPath).mode & 0o777, 0o644, "generated parser mode must be 0644");
}
execFileSync("git", ["diff", "--quiet", "--", "syntaxes/tree-sitter-simi.wasm"], {
  cwd: extensionPath,
});
