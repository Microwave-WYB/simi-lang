import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { test } from "node:test";

const require = createRequire(import.meta.url);
const { createOnigScanner, createOnigString, loadWASM } = require("vscode-oniguruma");
const { Registry, parseRawGrammar } = require("vscode-textmate");

const root = new URL("../", import.meta.url);
const grammarUrl = new URL("syntaxes/simi.tmLanguage.json", root);
const fixtureUrl = new URL("test/fixtures/highlighting.simi", root);

async function loadGrammar() {
  const wasmPath = require.resolve("vscode-oniguruma/release/onig.wasm");
  const wasm = await readFile(wasmPath);
  const arrayBuffer = wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength);
  await loadWASM(arrayBuffer);

  const registry = new Registry({
    onigLib: Promise.resolve({ createOnigScanner, createOnigString }),
    loadGrammar: async (scopeName) => {
      assert.equal(scopeName, "source.simi");
      return parseRawGrammar(await readFile(grammarUrl, "utf8"), grammarUrl.pathname);
    },
  });
  return registry.loadGrammar("source.simi");
}

function tokenize(grammar, source) {
  let ruleStack = null;
  return source.split("\n").map((line) => {
    const result = grammar.tokenizeLine(line, ruleStack);
    ruleStack = result.ruleStack;
    return { line, tokens: result.tokens };
  });
}

function scopesAt(lines, lineNumber, needle, occurrence = 1) {
  const entry = lines[lineNumber - 1];
  let column = -1;
  for (let found = 0, from = 0; found < occurrence; found += 1) {
    column = entry.line.indexOf(needle, from);
    assert.notEqual(column, -1, `missing ${JSON.stringify(needle)} on line ${lineNumber}`);
    from = column + needle.length;
  }
  const token = entry.tokens.find(({ startIndex, endIndex }) => startIndex <= column && column < endIndex);
  assert.ok(token, `no token for ${JSON.stringify(needle)} on line ${lineNumber}`);
  return token.scopes;
}

function assertScope(lines, lineNumber, needle, expected, occurrence) {
  assert.ok(
    scopesAt(lines, lineNumber, needle, occurrence).includes(expected),
    `${JSON.stringify(needle)} on line ${lineNumber} should have scope ${expected}`,
  );
}

test("TextMate grammar assigns semantic scopes to representative Simi syntax", async () => {
  const grammar = await loadGrammar();
  assert.ok(grammar, "source.simi grammar should load");
  const source = await readFile(fixtureUrl, "utf8");
  const lines = tokenize(grammar, source);

  assertScope(lines, 1, "Classify", "comment.line.double-dash.simi");
  assertScope(lines, 2, "classify", "entity.name.function.simi");
  assertScope(lines, 2, "value", "variable.parameter.simi");
  assertScope(lines, 3, "threshold", "variable.other.readwrite.simi");
  assertScope(lines, 3, "1.5e+2", "constant.numeric.float.simi");
  assertScope(lines, 4, "\\n", "constant.character.escape.simi");
  assertScope(lines, 5, "if", "keyword.control.conditional.simi");
  assertScope(lines, 5, ">=", "keyword.operator.comparison.simi");
  assertScope(lines, 5, "and", "keyword.operator.logical.simi");
  assertScope(lines, 8, "|>", "keyword.operator.pipeline.simi");
  assertScope(lines, 8, "tap", "storage.modifier.tap.simi");
  assertScope(lines, 8, "inspect", "support.function.builtin.simi");
  assertScope(lines, 11, "..", "keyword.operator.rest.simi");
  assertScope(lines, 11, "->", "keyword.control.case.arrow.simi");
  assertScope(lines, 17, "item", "variable.parameter.simi");
  assertScope(lines, 18, "require", "support.function.builtin.simi");
  assertScope(lines, 19, ".map", "punctuation.accessor.simi");
  assertScope(lines, 19, "map", "variable.other.property.simi", 2);
  assertScope(lines, 19, "<|", "keyword.operator.pipeline.simi");
  assertScope(lines, 20, "raise", "keyword.control.exception.simi");
  assertScope(lines, 23, "state", "variable.other.readwrite.simi");
  assertScope(lines, 26, "\\q", "invalid.illegal.escape.simi");
  assertScope(lines, 27, "is", "keyword.operator.comparison.simi");
  assertScope(lines, 27, "\"integer\"", "string.quoted.double.simi");
  assertScope(lines, 28, "is", "keyword.operator.comparison.simi");
  assertScope(lines, 28, "\"function\"", "string.quoted.double.simi");
});
