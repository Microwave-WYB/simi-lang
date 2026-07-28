import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

const root = new URL("../", import.meta.url);

async function json(path) {
  return JSON.parse(await readFile(new URL(path, root), "utf8"));
}

test("extension manifest associates .simi files with the TextMate grammar", async () => {
  const manifest = await json("package.json");
  const lockfile = await json("package-lock.json");
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
  assert.equal(manifest.version, "0.1.0-alpha.1");
  assert.equal(lockfile.version, manifest.version);
  assert.equal(lockfile.packages[""].version, manifest.version);
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

  assert.ok(
    configuration.autoClosingPairs.every(({ open, close }) => open.length === 1 && close.length === 1),
    "multi-character pairs require extension-managed token and close handling",
  );
  assert.equal(configuration.onEnterRules, undefined);

  const increase = new RegExp(configuration.indentationRules.increaseIndentPattern);
  const decrease = new RegExp(configuration.indentationRules.decreaseIndentPattern);
  const indentNext = new RegExp(configuration.indentationRules.indentNextLinePattern);
  for (const line of [
    "fn add(a, b)",
    "fn add(a, b) do",
    "if ready then",
    "case value of",
    "[head, ..tail] when ready =>",
    "[head, ..tail] when ready => do",
    "catch of",
    "else",
  ]) {
    assert.match(line, increase);
  }
  for (const line of ["end", "elseif ready then", "else", "catch"]) {
    assert.match(line, decrease);
  }
  for (const line of ["case n of", "    case n of -- comment", "catch of", "_ =>"]) {
    assert.match(line, indentNext);
    assert.match(line, increase);
  }
  for (const oneLine of [
    "fn add(a, b) do a + b end",
    "_ => do value end",
    "case n of _ => do n end",
    'case "x of y" of _ => do 1 end',
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
    "case value of",
    "1 =>",
    "    first()",
    "2 =>",
    "    second()",
    "end",
  ];
  const levels = [0];
  for (let index = 1; index < lines.length; index += 1) {
    levels.push(nextIndent(levels[index - 1], lines[index - 1], lines[index]));
  }
  assert.deepEqual(
    levels,
    [0, 1, 2, 2, 3, 2],
    "line-regex fallback cannot retain the enclosing case contribution",
  );
  const structuralCaseTarget = [0, 1, 2, 1, 2, 0];
  assert.notDeepEqual(
    levels,
    structuralCaseTarget,
    "declarative indentation must not be represented as sufficient for the structural target",
  );

  const protectedLines = [
    "do",
    "    prepare()",
    "catch of",
    "first =>",
    "    recover_first()",
    "second =>",
    "    recover_second()",
    "end",
  ];
  const protectedLevels = [0];
  for (let index = 1; index < protectedLines.length; index += 1) {
    protectedLevels.push(nextIndent(protectedLevels[index - 1], protectedLines[index - 1], protectedLines[index]));
  }
  assert.deepEqual(
    protectedLevels,
    [0, 1, 0, 1, 2, 2, 3, 2],
    "line-regex fallback likewise cannot retain the protected-expression contribution",
  );
  const structuralCatchTarget = [0, 1, 0, 1, 2, 1, 2, 0];
  assert.notDeepEqual(
    protectedLevels,
    structuralCatchTarget,
    "declarative indentation must not be represented as sufficient for catch arms",
  );

  assert.match("_ =>", increase);
  assert.doesNotMatch("-- fake =>", increase);
  assert.doesNotMatch("value -- fake =>", increase);
  assert.match("_ => do", increase);
  assert.match("    case nested of", indentNext);
  assert.doesNotMatch("do operation() catch of _ => value end", increase);

  for (const legacyLine of ["match value with", "case value ->"]) {
    assert.doesNotMatch(legacyLine, increase);
    assert.doesNotMatch(legacyLine, decrease);
    assert.doesNotMatch(legacyLine, indentNext);
  }
});

test("control-flow snippets use empty numeric tab stops without defaults", async () => {
  const snippets = await json("snippets/simi.json");
  const byPrefix = Object.fromEntries(
    Object.values(snippets).map((snippet) => [snippet.prefix, snippet]),
  );

  assert.deepEqual(Object.keys(byPrefix).sort(), [
    "case", "do", "fn",
  ]);
  assert.deepEqual(byPrefix.case.body, [
    "case ${1} of",
    "    ${2}",
    "end",
  ]);
  assert.equal(byPrefix.case.body.filter((line) => line === "end").length, 1);
  assert.deepEqual(byPrefix.fn.body, [
    "fn ${1}(${2}) ${3}",
  ]);
  assert.deepEqual(byPrefix.do.body, [
    "do",
    "    ${1}",
    "end",
  ]);

  for (const snippet of Object.values(snippets)) {
    assert.ok(
      snippet.body.every((line) => !/\$\{\d+:[^}]+\}/.test(line)),
      `${snippet.prefix} must use blank tab stops without visible placeholder defaults`,
    );
    assert.ok(
      snippet.body.every((line) => !/\$0/.test(line)),
      `${snippet.prefix} must use numeric-only tab stops without $0`,
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
    "case", "of", "when", "raise", "catch",
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
  assert.ok(operatorInventory.includes("=>"), "pattern-result arrow must be scoped");
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
