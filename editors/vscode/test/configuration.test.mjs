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
  assert.equal(manifest.main, "./src/extension.js");
  assert.deepEqual(manifest.activationEvents, ["onLanguage:simi"]);
  assert.deepEqual(manifest.extensionKind, ["workspace"]);
  assert.equal(
    manifest.contributes.configuration.properties["simi.languageServer.path"].scope,
    "machine-overridable",
  );
  assert.ok(
    manifest.contributes.commands.some(
      ({ command }) => command === "simi.restartLanguageServer",
    ),
  );
  assert.equal(manifest.dependencies["vscode-languageclient"], "9.0.1");
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
    "of [head, ..tail] when ready do",
    "catch _ do",
    "else",
    "let result = try",
  ]) {
    assert.match(line, increase);
  }
  for (const line of ["end", "elseif ready then", "else", "of _ do", "catch _ do"]) {
    assert.match(line, decrease);
  }
  assert.doesNotMatch("of _ do value end", increase, "one-line clauses must not indent the following line");
  for (const legacyLine of ["match value with", "case value ->"]) {
    assert.doesNotMatch(legacyLine, increase);
    assert.doesNotMatch(legacyLine, decrease);
  }
});

test("grammar keyword inventory follows the current Simi lexer", async () => {
  const grammar = await json("syntaxes/simi.tmLanguage.json");
  const keywordPatterns = grammar.repository.keywords.patterns.map(({ match }) => match).join("\n");
  const keywordInventory = keywordPatterns.replaceAll("\\b", "");
  const lexerKeywords = [
    "fn", "do", "end", "if", "then", "elseif", "else", "let", "tap", "and", "or", "not",
    "loop", "break", "continue", "case", "of", "when", "raise", "try", "catch",
  ];

  for (const keyword of lexerKeywords) {
    assert.match(keywordInventory, new RegExp(`\\b${keyword}\\b`), `grammar should contain lexer keyword ${keyword}`);
  }
  for (const removed of ["match", "with", "is"]) {
    assert.doesNotMatch(keywordInventory, new RegExp(`\\b${removed}\\b`));
  }
  const operatorInventory = grammar.repository.operators.patterns
    .map(({ match }) => match)
    .join("\n")
    .replaceAll("\\b", "");
  assert.ok(!operatorInventory.includes("->"), "legacy clause arrow must not be scoped");
  assert.match(operatorInventory, /\\\?>/, "nil-aware pipeline must be scoped");
  assert.match(operatorInventory, /\\\?/, "nil propagation must be scoped");
  assert.doesNotMatch(
    operatorInventory,
    /\bis\b/,
    "ordinary identifier is must not be scoped as an operator",
  );
  assert.match(
    grammar.repository.builtins.patterns[0].match,
    /type/,
    "type calls should retain builtin highlighting",
  );
});
