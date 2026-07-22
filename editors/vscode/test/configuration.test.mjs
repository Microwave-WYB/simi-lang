import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

const root = new URL("../", import.meta.url);

async function json(path) {
  return JSON.parse(await readFile(new URL(path, root), "utf8"));
}

test("extension manifest associates .simi files with the TextMate grammar", async () => {
  const manifest = await json("package.json");
  const language = manifest.contributes.languages.find(({ id }) => id === "simi");
  const grammar = manifest.contributes.grammars.find(({ language }) => language === "simi");

  assert.ok(language, "simi language contribution should exist");
  assert.deepEqual(language.extensions, [".simi"]);
  assert.equal(language.configuration, "./language-configuration.json");
  assert.ok(grammar, "simi grammar contribution should exist");
  assert.equal(grammar.scopeName, "source.simi");
  assert.equal(grammar.path, "./syntaxes/simi.tmLanguage.json");
  assert.equal(manifest.scripts.prepackage, "npm test");
  assert.equal(manifest.scripts.package, "vsce package");
  assert.equal(manifest.scripts.publish, "vsce publish");
});

test("language configuration covers comments, pairs, indentation, and folding", async () => {
  const configuration = await json("language-configuration.json");

  assert.equal(configuration.comments.lineComment, "--");
  assert.deepEqual(configuration.brackets, [
    ["{", "}"],
    ["[", "]"],
    ["(", ")"],
  ]);
  assert.ok(configuration.autoClosingPairs.some(({ open, close }) => open === '"' && close === '"'));
  assert.ok(configuration.surroundingPairs.some(([open, close]) => open === '"' && close === '"'));
  assert.doesNotThrow(() => new RegExp(configuration.wordPattern));
  assert.doesNotThrow(() => new RegExp(configuration.indentationRules.increaseIndentPattern));
  assert.doesNotThrow(() => new RegExp(configuration.indentationRules.decreaseIndentPattern));
  assert.doesNotThrow(() => new RegExp(configuration.folding.markers.start));
  assert.doesNotThrow(() => new RegExp(configuration.folding.markers.end));

  const increase = new RegExp(configuration.indentationRules.increaseIndentPattern);
  const decrease = new RegExp(configuration.indentationRules.decreaseIndentPattern);
  for (const line of [
    "fn add(a, b) do",
    "if ready then",
    "match value with",
    "case value ->",
    "else",
    "let result = try",
  ]) {
    assert.match(line, increase);
  }
  for (const line of ["end", "elseif ready then", "else", "catch", "case value ->"]) {
    assert.match(line, decrease);
  }
});

test("grammar keyword inventory follows the current Simi lexer", async () => {
  const grammarSource = await readFile(new URL("syntaxes/simi.tmLanguage.json", root), "utf8");
  const lexerKeywords = [
    "fn", "do", "end", "if", "then", "elseif", "else", "let", "tap", "nil", "true",
    "false", "and", "or", "not", "loop", "break", "continue", "match", "with", "case",
    "when", "raise", "try", "catch",
  ];

  for (const keyword of lexerKeywords) {
    assert.match(grammarSource, new RegExp(`\\b${keyword}\\b`), `grammar should contain lexer keyword ${keyword}`);
  }
});
