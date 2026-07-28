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

function assertNotScope(lines, lineNumber, needle, unexpected, occurrence) {
  assert.ok(
    !scopesAt(lines, lineNumber, needle, occurrence).includes(unexpected),
    `${JSON.stringify(needle)} on line ${lineNumber} should not have scope ${unexpected}`,
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
  assertScope(lines, 3, "0x_ff", "constant.numeric.integer.simi");
  assertScope(lines, 3, "1_000.25", "constant.numeric.float.simi");
  assertScope(lines, 3, "1.5_0e+2", "constant.numeric.float.simi");
  assertScope(lines, 4, "\\n", "constant.character.escape.simi");
  assertScope(lines, 5, "if", "keyword.control.conditional.simi");
  assertScope(lines, 5, ">=", "keyword.operator.comparison.simi");
  assertScope(lines, 5, "and", "keyword.operator.logical.simi");
  assertScope(lines, 8, "?>", "keyword.operator.pipeline.simi");
  assertScope(lines, 8, "tap", "storage.modifier.tap.simi");
  assertScope(lines, 8, "inspect", "support.function.builtin.simi");
  assertScope(lines, 10, "case", "keyword.control.case.simi");
  assertScope(lines, 10, "of", "keyword.control.case.simi");
  assertScope(lines, 11, "..", "keyword.operator.rest.simi");
  assertScope(lines, 11, "when", "keyword.control.case.simi");
  assertScope(lines, 11, "=>", "keyword.operator.arm.simi");
  assertScope(lines, 13, "=>", "keyword.operator.arm.simi");
  assertScope(lines, 15, "end", "keyword.control.block.simi");
  assertScope(lines, 20, "integer", "support.type.primitive.simi");
  assertScope(lines, 20, "..", "keyword.operator.rest.simi");
  assertScope(lines, 24, "item", "variable.parameter.simi");
  assertScope(lines, 25, "require", "support.function.builtin.simi");
  assertScope(lines, 26, ".map", "punctuation.accessor.simi");
  assertScope(lines, 26, "map", "variable.other.property.simi", 2);
  assertScope(lines, 26, "<|", "keyword.operator.pipeline.simi");
  assertScope(lines, 29, "raise", "keyword.control.exception.simi");
  assertScope(lines, 30, "catch", "keyword.control.exception.simi");
  assertScope(lines, 30, "of", "keyword.control.case.simi");
  assertScope(lines, 31, "=>", "keyword.operator.arm.simi");
  assertScope(lines, 34, "?", "keyword.operator.pipeline.simi");
  assertScope(lines, 36, "state", "variable.other.simi");
  assertScope(lines, 36, "break", "variable.other.property.simi");
  assertScope(lines, 36, "continue", "variable.other.property.simi");
  assertScope(lines, 40, "\\q", "invalid.illegal.escape.simi");
  assertScope(lines, 41, "<>", "keyword.operator.concatenation.simi");
  assertScope(lines, 42, "type", "support.function.builtin.simi");
  assertScope(lines, 42, "==", "keyword.operator.comparison.simi");
  assertScope(lines, 45, "|>", "keyword.operator.pipeline.simi");
  assertScope(lines, 42, "\"integer\"", "string.quoted.double.simi");
  assertScope(lines, 43, "type", "support.function.builtin.simi");
  assertScope(lines, 43, "==", "keyword.operator.comparison.simi");
  assertScope(lines, 43, "\"function\"", "string.quoted.double.simi");
  assertScope(lines, 44, "is", "variable.other.readwrite.simi");
  assertScope(lines, 46, "identity", "entity.name.function.simi");
  assertScope(lines, 46, "'a", "variable.other.generic-type.simi");
  assertScope(lines, 46, "integer", "support.type.primitive.simi");
  assertScope(lines, 46, "value", "variable.parameter.simi");
  assertScope(lines, 46, "!", "keyword.operator.type.simi");
  assertScope(lines, 47, "!", "keyword.operator.type.simi");
  assertScope(lines, 48, "'a", "variable.other.generic-type.simi");
  assertScope(lines, 48, "!", "keyword.operator.type.simi");
  assertScope(lines, 51, "nested", "entity.name.function.simi");
  assertScope(lines, 52, "!", "keyword.operator.type.simi");
  assertScope(lines, 53, "!", "keyword.operator.type.simi");
  assertScope(lines, 54, "string", "variable.other.simi");
  assertNotScope(lines, 54, "string", "support.type.primitive.simi");
  assertScope(lines, 54, "to_number", "variable.other.property.simi");
  assertScope(lines, 55, "!=", "keyword.operator.comparison.simi");
  assertNotScope(lines, 55, "!=", "keyword.operator.type.simi");
  assertScope(lines, 57, "requires", "keyword.declaration.simi");
});
