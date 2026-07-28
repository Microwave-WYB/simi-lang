import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Language, Parser } from "web-tree-sitter";

const extensionPath = fileURLToPath(new URL("../", import.meta.url));
const parserPath = fileURLToPath(new URL("../syntaxes/tree-sitter-simi.wasm", import.meta.url));
const parserBytes = readFileSync(parserPath);

assert.ok(WebAssembly.validate(parserBytes), "bundled parser must be valid WebAssembly");
execFileSync("git", ["ls-files", "--error-unmatch", "syntaxes/tree-sitter-simi.wasm"], {
  cwd: extensionPath,
  stdio: "ignore",
});
execFileSync("git", ["diff", "--quiet", "HEAD", "--", "syntaxes/tree-sitter-simi.wasm"], {
  cwd: extensionPath,
});

await Parser.init();
const language = await Language.load(parserPath);
const parser = new Parser();
parser.setLanguage(language);
const tree = parser.parse("let value = 1");
assert.equal(tree.rootNode.hasError, false, "bundled parser must load and parse Simi source");
