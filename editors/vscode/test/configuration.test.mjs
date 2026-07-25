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
  assert.deepEqual(manifest.contributes.snippets, [
    { language: "simi", path: "./snippets/simi.json" },
  ]);
  assert.equal(manifest.main, "./src/extension.js");
  assert.equal(manifest.license, "MIT");
  assert.equal(manifest.repository.url, "https://github.com/Microwave-WYB/simi-lang.git");
  assert.equal(manifest.repository.directory, "editors/vscode");
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
  assert.doesNotThrow(() => new RegExp(configuration.indentationRules.indentNextLinePattern));
  assert.doesNotThrow(() => new RegExp(configuration.folding.markers.start));
  assert.doesNotThrow(() => new RegExp(configuration.folding.markers.end));

  const increase = new RegExp(configuration.indentationRules.increaseIndentPattern);
  const decrease = new RegExp(configuration.indentationRules.decreaseIndentPattern);
  const indentNext = new RegExp(configuration.indentationRules.indentNextLinePattern);
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
  for (const line of ["case n", "    case n -- comment", 'case "x of y"']) {
    assert.match(line, indentNext);
    assert.doesNotMatch(line, increase, "case indentation must affect only its next line");
  }
  for (const oneLine of [
    "of _ do value end",
    "case n of _ do n end",
    'case "x of y" of _ do 1 end',
  ]) {
    assert.doesNotMatch(oneLine, increase, "one-line forms must not indent the following line");
    assert.doesNotMatch(oneLine, indentNext, "complete cases must not indent the following line");
  }

  const nextIndent = (previousIndent, previousLine, currentLine) => {
    const inherited = previousIndent
      + (increase.test(previousLine) || indentNext.test(previousLine) ? 1 : 0);
    return Math.max(0, inherited - (decrease.test(currentLine) ? 1 : 0));
  };
  const lines = [
    "case value",
    "of 1 do",
    "    first()",
    "of 2 do",
    "    second()",
    "end",
  ];
  const levels = [0];
  for (let index = 1; index < lines.length; index += 1) {
    levels.push(nextIndent(levels[index - 1], lines[index - 1], lines[index]));
  }
  assert.deepEqual(
    levels,
    [0, 0, 1, 0, 1, 0],
    "each sibling of and the final end must align with case",
  );

  for (const legacyLine of ["match value with", "case value ->"]) {
    assert.doesNotMatch(legacyLine, increase);
    assert.doesNotMatch(legacyLine, decrease);
    assert.doesNotMatch(legacyLine, indentNext);
  }
});

test("control-flow snippets use construct-specific final ends", async () => {
  const snippets = await json("snippets/simi.json");
  const byPrefix = Object.fromEntries(
    Object.values(snippets).map((snippet) => [snippet.prefix, snippet]),
  );

  assert.deepEqual(Object.keys(byPrefix).sort(), [
    "case", "do", "fn", "fnexpr", "if", "ifelse", "loop", "try",
  ]);
  assert.deepEqual(byPrefix.case.body, [
    "case $1",
    "of $2 do",
    "    $3",
    "of _ do",
    "    $0",
    "end",
  ]);
  assert.equal(byPrefix.case.body.filter((line) => line === "end").length, 1);
  assert.deepEqual(byPrefix.try.body, [
    "try",
    "    $1",
    "catch $2 do",
    "    $0",
    "end",
  ]);
  assert.equal(byPrefix.try.body.filter((line) => line === "end").length, 1);
  assert.ok(!byPrefix.of, "case clauses must not insert their own end");
  assert.ok(!byPrefix.catch, "catch clauses must not insert their own end");
  for (const snippet of Object.values(snippets)) {
    assert.equal(snippet.body.at(-1), "end", `${snippet.prefix} must own one final end`);
    assert.ok(
      snippet.body.every((line) => !/\$\{\d+:[^}]+\}/.test(line)),
      `${snippet.prefix} must use blank tab stops rather than visible placeholder defaults`,
    );
    assert.equal(typeof snippet.description, "string");
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
  assert.ok(operatorInventory.includes("->"), "type return arrow must be scoped");
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
