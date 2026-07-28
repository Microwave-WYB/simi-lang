"use strict";

const path = require("node:path");
const { Language, Parser } = require("web-tree-sitter");

const WORD_CHARACTER = /[A-Za-z0-9_]/;
const ENTER_INSERTION = /^(?:\r\n|\n)([ \t]*)$/;
const PARSE_BUDGET_MILLISECONDS = 20;

function lineEndsWithCodeDo(text, cursor) {
  if (cursor !== text.length) {
    return false;
  }

  let tokenEnd = cursor;
  while (tokenEnd > 0 && /[ \t]/.test(text[tokenEnd - 1])) {
    tokenEnd -= 1;
  }
  const tokenStart = tokenEnd - 2;
  if (tokenStart < 0 || text.slice(tokenStart, tokenEnd) !== "do") {
    return false;
  }
  if (tokenStart > 0 && WORD_CHARACTER.test(text[tokenStart - 1])) {
    return false;
  }

  let inString = false;
  for (let index = 0; index < tokenStart; index += 1) {
    const character = text[index];
    if (inString) {
      if (character === "\\") {
        index += 1;
      } else if (character === "\"") {
        inString = false;
      }
    } else if (character === "\"") {
      inString = true;
    } else if (character === "-" && text[index + 1] === "-") {
      return false;
    }
  }
  return !inString;
}

function leadingWhitespace(text) {
  return text.match(/^[ \t]*/)[0];
}

function indentationUnit(options) {
  const tabSize = Number.isInteger(options.tabSize) ? options.tabSize : 4;
  return options.insertSpaces === false ? "\t" : " ".repeat(tabSize);
}

class GeneratedCloserStore {
  constructor() {
    this.byDocument = new WeakMap();
  }

  add(document, start, end) {
    const marker = { start, end };
    const markers = this.byDocument.get(document) ?? [];
    markers.push(marker);
    this.byDocument.set(document, markers);
    return marker;
  }

  at(document, offset) {
    return (this.byDocument.get(document) ?? []).find(({ start }) => start === offset);
  }

  contains(document, marker) {
    return (this.byDocument.get(document) ?? []).includes(marker);
  }

  remove(document, marker) {
    const markers = this.byDocument.get(document) ?? [];
    const remaining = markers.filter((candidate) => candidate !== marker);
    if (remaining.length === 0) {
      this.byDocument.delete(document);
    } else {
      this.byDocument.set(document, remaining);
    }
  }

  prepareConsumption(marker, length) {
    marker.start += length;
  }

  applyChanges(document, contentChanges) {
    const markers = this.byDocument.get(document);
    if (!markers) {
      return;
    }

    const changes = [...contentChanges].sort((left, right) => right.rangeOffset - left.rangeOffset);
    const invalid = new Set();
    for (const change of changes) {
      const changeStart = change.rangeOffset;
      const changeEnd = changeStart + change.rangeLength;
      const delta = change.text.length - change.rangeLength;
      for (const marker of markers) {
        if (invalid.has(marker)) {
          continue;
        }
        if (changeEnd <= marker.start) {
          marker.start += delta;
          marker.end += delta;
        } else if (changeStart >= marker.end) {
          // The generated closer is unchanged.
        } else {
          invalid.add(marker);
        }
      }
    }

    const remaining = markers.filter((marker) => !invalid.has(marker));
    if (remaining.length === 0) {
      this.byDocument.delete(document);
    } else {
      this.byDocument.set(document, remaining);
    }
  }

  clear() {
    this.byDocument = new WeakMap();
  }
}

function createDoEndTypingController({ vscode }) {
  const closers = new GeneratedCloserStore();
  const internalEdits = new WeakSet();
  let pendingEnterPlans = new WeakMap();

  function activeSingleCursorEditor(document) {
    const editor = vscode.window.activeTextEditor;
    if (
      !editor
      || editor.document !== document
      || document.languageId !== "simi"
      || editor.selections.length !== 1
      || !editor.selection.isEmpty
    ) {
      return undefined;
    }
    return editor;
  }

  function observedEnterPlan(document, contentChanges) {
    if (contentChanges.length !== 1) {
      return undefined;
    }
    const change = contentChanges[0];
    const insertedLineBreak = /^(?:\r\n|\n)([ \t]*)$/.exec(change.text);
    if (change.rangeLength !== 0 || !insertedLineBreak) {
      return undefined;
    }

    const editor = activeSingleCursorEditor(document);
    if (!editor) {
      return undefined;
    }
    const line = document.lineAt(change.range.start.line);
    const prefix = line.text.slice(0, change.range.start.character);
    if (line.text !== prefix || !lineEndsWithCodeDo(prefix, prefix.length)) {
      return undefined;
    }

    const selection = editor.selection;
    if (
      selection.active.line !== change.range.start.line
      || selection.active.character !== change.range.start.character
    ) {
      return undefined;
    }

    const baseIndent = leadingWhitespace(prefix);
    const bodyLine = change.range.start.line + 1;
    const insertedIndent = insertedLineBreak[1];
    return {
      document,
      editor,
      bodyLine,
      insertedIndent,
      baseIndent,
      childIndent: baseIndent + indentationUnit(editor.options),
    };
  }

  function observedOvertypePlan(document, contentChanges) {
    if (contentChanges.length !== 1) {
      return undefined;
    }
    const change = contentChanges[0];
    if (
      change.rangeLength !== 0
      || change.text.length === 0
      || change.text.includes("\n")
      || change.text.includes("\r")
    ) {
      return undefined;
    }

    const editor = activeSingleCursorEditor(document);
    if (!editor) {
      return undefined;
    }
    const insertionEnd = change.rangeOffset + change.text.length;
    const marker = closers.at(document, insertionEnd);
    if (!marker || insertionEnd + change.text.length > marker.end) {
      return undefined;
    }
    const range = new vscode.Range(
      document.positionAt(insertionEnd),
      document.positionAt(insertionEnd + change.text.length),
    );
    if (document.getText(range) !== change.text) {
      return undefined;
    }
    return { document, editor, marker, length: change.text.length, range };
  }

  async function consumeGeneratedCloser(plan) {
    const { document, editor, marker, length, range } = plan;
    if (vscode.window.activeTextEditor !== editor || !closers.contains(document, marker)) {
      return;
    }

    closers.prepareConsumption(marker, length);
    internalEdits.add(document);
    try {
      const edited = await editor.edit(
        (builder) => builder.delete(range),
        { undoStopBefore: false, undoStopAfter: false },
      );
      if (!edited || !closers.contains(document, marker)) {
        closers.remove(document, marker);
        return;
      }
      if (marker.start === marker.end) {
        closers.remove(document, marker);
      }
    } finally {
      internalEdits.delete(document);
    }
  }

  async function insertBlockShell(plan) {
    const { document, editor, bodyLine, insertedIndent, baseIndent, childIndent } = plan;
    if (vscode.window.activeTextEditor !== editor || editor.document !== document) {
      return;
    }
    const selection = editor.selection;
    if (
      editor.selections.length !== 1
      || !selection.isEmpty
      || selection.active.line !== bodyLine
      || selection.active.character !== insertedIndent.length
    ) {
      return;
    }

    const bodyLineText = document.lineAt(bodyLine).text;
    if (bodyLineText !== insertedIndent) {
      return;
    }

    const bodyStart = new vscode.Position(bodyLine, 0);
    const replaceRange = new vscode.Range(
      bodyStart,
      new vscode.Position(bodyLine, insertedIndent.length),
    );
    const replacement = `${childIndent}\n${baseIndent}end`;
    internalEdits.add(document);
    try {
      const edited = await editor.edit(
        (builder) => builder.replace(replaceRange, replacement),
        { undoStopBefore: false, undoStopAfter: false },
      );
      if (!edited) {
        return;
      }
    } finally {
      internalEdits.delete(document);
    }

    const bodyCursor = new vscode.Position(bodyLine, childIndent.length);
    editor.selection = new vscode.Selection(bodyCursor, bodyCursor);
    const closeStart = new vscode.Position(bodyLine + 1, baseIndent.length);
    closers.add(document, document.offsetAt(closeStart), document.offsetAt(closeStart) + 3);
  }

  async function onDidChangeTextDocument(event) {
    closers.applyChanges(event.document, event.contentChanges);
    pendingEnterPlans.delete(event.document);
    if (internalEdits.has(event.document)) {
      return;
    }

    const overtypePlan = observedOvertypePlan(event.document, event.contentChanges);
    if (overtypePlan) {
      await consumeGeneratedCloser(overtypePlan);
      return;
    }

    const enterPlan = observedEnterPlan(event.document, event.contentChanges);
    if (enterPlan) {
      pendingEnterPlans.set(event.document, enterPlan);
    }
  }

  async function onDidChangeTextEditorSelection(event) {
    const { textEditor: editor } = event;
    const { document } = editor;
    const plan = pendingEnterPlans.get(document);
    if (!plan || plan.editor !== editor) {
      return;
    }

    pendingEnterPlans.delete(document);
    const selection = event.selections[0];
    if (
      event.selections.length !== 1
      || !selection.isEmpty
      || selection.active.line !== plan.bodyLine
      || selection.active.character !== plan.insertedIndent.length
    ) {
      return;
    }

    await insertBlockShell(plan);
  }

  return {
    clear() {
      closers.clear();
      pendingEnterPlans = new WeakMap();
    },
    onDidChangeTextDocument,
    onDidChangeTextEditorSelection,
  };
}

async function createSimiParser(extensionPath) {
  await Parser.init();
  const language = await Language.load(
    path.join(extensionPath, "syntaxes", "tree-sitter-simi.wasm"),
  );
  const parser = new Parser();
  parser.setLanguage(language);
  return parser;
}

function visitNamed(node, callback) {
  const pending = [node];
  while (pending.length > 0) {
    const current = pending.pop();
    if (callback(current)) {
      return current;
    }
    for (let index = current.namedChildren.length - 1; index >= 0; index -= 1) {
      pending.push(current.namedChildren[index]);
    }
  }
  return undefined;
}

function parseWithinBudget(parser, source) {
  const started = Date.now();
  const tree = parser.parse(source, null, {
    progressCallback: () => Date.now() - started >= PARSE_BUDGET_MILLISECONDS,
  });
  if (!tree) {
    parser.reset();
  }
  return tree;
}

function parsedArmTarget(parser, source, headerLine, insertionOffset) {
  const endCount = Math.max(
    1,
    source.match(/\b(?:case|do|fn|if)\b/g)?.length ?? 0,
  );
  const ends = "end\n".repeat(endCount);
  const completions = ["", ends, `nil\n${ends}`];

  for (const completion of completions) {
    const candidate = completion.length === 0
      ? source
      : source.slice(0, insertionOffset) + completion + source.slice(insertionOffset);
    const tree = parseWithinBudget(parser, candidate);
    if (!tree) {
      continue;
    }
    try {
      const arm = visitNamed(tree.rootNode, (node) => {
        if (
          !["case_clause", "catch_arm"].includes(node.type)
          || node.startPosition.row !== headerLine
          || node.hasError
        ) {
          return false;
        }
        const owner = node.parent;
        return Boolean(
          owner
          && !owner.hasError
          && (
            (node.type === "case_clause" && owner.type === "case_expression")
            || (node.type === "catch_arm" && owner.type === "protected_expression")
          ),
        );
      });
      if (!arm) {
        continue;
      }

      const body = arm.childForFieldName("body");
      if (!body) {
        continue;
      }
      const blockContinues = body.type === "block_expression"
        && body.endPosition.row > headerLine;
      const bodyStartsLater = body.startPosition.row > headerLine;
      return {
        ownerLine: arm.parent.startPosition.row,
        bodyIndent: blockContinues || bodyStartsLater,
        doHeader: blockContinues && body.startPosition.row === headerLine,
      };
    } finally {
      tree.delete();
    }
  }
  return undefined;
}

function parsedFinalEndTarget(parser, source, line) {
  const text = source.split(/\r?\n/)[line] ?? "";
  if (!/^\s*end\s*(?:--.*)?$/.test(text)) {
    return undefined;
  }

  const tree = parseWithinBudget(parser, source);
  if (!tree) {
    return undefined;
  }
  try {
    let best;
    visitNamed(tree.rootNode, (node) => {
      if (
        ["case_expression", "protected_expression"].includes(node.type)
        && node.endPosition.row === line
        && !node.hasError
        && (!best || node.startPosition.row > best.startPosition.row)
      ) {
        best = node;
      }
      return false;
    });
    return best ? { ownerLine: best.startPosition.row } : undefined;
  } finally {
    tree.delete();
  }
}

function createStructuralIndentTypingController({ vscode, parser }) {
  const internalEdits = new WeakSet();
  let pendingPlans = new WeakMap();

  function activeSingleCursorEditor(document) {
    const editor = vscode.window.activeTextEditor;
    if (
      !editor
      || editor.document !== document
      || document.languageId !== "simi"
      || editor.selections.length !== 1
      || !editor.selection.isEmpty
    ) {
      return undefined;
    }
    return editor;
  }

  function observedPlan(document, contentChanges) {
    if (contentChanges.length !== 1) {
      return undefined;
    }
    const change = contentChanges[0];
    const insertedLineBreak = ENTER_INSERTION.exec(change.text);
    if (change.rangeLength !== 0 || !insertedLineBreak) {
      return undefined;
    }

    const editor = activeSingleCursorEditor(document);
    if (!editor) {
      return undefined;
    }
    const headerLine = change.range.start.line;
    const line = document.lineAt(headerLine);
    if (change.range.start.character !== line.text.length) {
      return undefined;
    }
    const selection = editor.selection;
    if (
      selection.active.line !== headerLine
      || selection.active.character !== change.range.start.character
    ) {
      return undefined;
    }

    const source = document.getText();
    const insertedIndent = insertedLineBreak[1];
    const bodyLine = headerLine + 1;
    const insertionOffset = change.rangeOffset + change.text.length;
    const arm = /^\s*of\b/.test(line.text)
      ? parsedArmTarget(parser, source, headerLine, insertionOffset)
      : undefined;
    if (arm) {
      const ownerIndent = leadingWhitespace(document.lineAt(arm.ownerLine).text);
      const unit = indentationUnit(editor.options);
      const headerIndent = ownerIndent + unit;
      return {
        document,
        editor,
        headerLine,
        bodyLine,
        insertedIndent,
        headerIndent,
        nextIndent: arm.bodyIndent ? headerIndent + unit : headerIndent,
        doHeader: arm.doHeader,
      };
    }

    const finalEnd = parsedFinalEndTarget(parser, source, headerLine);
    if (!finalEnd) {
      return undefined;
    }
    const ownerIndent = leadingWhitespace(document.lineAt(finalEnd.ownerLine).text);
    return {
      document,
      editor,
      headerLine,
      bodyLine,
      insertedIndent,
      headerIndent: ownerIndent,
      nextIndent: ownerIndent,
      doHeader: false,
    };
  }

  function observedTypedEndPlan(document, contentChanges) {
    if (contentChanges.length !== 1) {
      return undefined;
    }
    const change = contentChanges[0];
    if (
      change.rangeLength !== 0
      || change.text.length === 0
      || change.text.includes("\n")
      || change.text.includes("\r")
    ) {
      return undefined;
    }

    const editor = activeSingleCursorEditor(document);
    if (!editor) {
      return undefined;
    }
    const lineNumber = change.range.start.line;
    const line = document.lineAt(lineNumber);
    if (
      change.range.start.character + change.text.length !== line.text.length
      || !/^\s*end$/.test(line.text)
    ) {
      return undefined;
    }
    const selection = editor.selection;
    if (
      selection.active.line !== lineNumber
      || selection.active.character !== change.range.start.character
    ) {
      return undefined;
    }

    const finalEnd = parsedFinalEndTarget(parser, document.getText(), lineNumber);
    if (!finalEnd) {
      return undefined;
    }
    const ownerIndent = leadingWhitespace(document.lineAt(finalEnd.ownerLine).text);
    return {
      document,
      editor,
      headerLine: lineNumber,
      bodyLine: undefined,
      insertedIndent: undefined,
      headerIndent: ownerIndent,
      nextIndent: undefined,
      doHeader: false,
      expectedLine: lineNumber,
      expectedCharacter: change.range.start.character + change.text.length,
      finalContentLength: line.text.length - leadingWhitespace(line.text).length,
    };
  }

  async function applyPlan(plan) {
    const {
      document,
      editor,
      headerLine,
      bodyLine,
      insertedIndent,
      headerIndent,
      nextIndent,
      doHeader,
      finalContentLength,
    } = plan;
    if (vscode.window.activeTextEditor !== editor || editor.document !== document) {
      return;
    }
    const selection = editor.selection;
    const generatedDoShell = bodyLine !== undefined
      && doHeader
      && document.lineAt(bodyLine).text.trim() === ""
      && document.lineAt(bodyLine + 1).text.trim() === "end";
    const expectedLine = plan.expectedLine ?? bodyLine;
    const expectedCharacter = plan.expectedCharacter ?? (generatedDoShell
      ? leadingWhitespace(document.lineAt(bodyLine).text).length
      : insertedIndent.length);
    if (
      editor.selections.length !== 1
      || !selection.isEmpty
      || selection.active.line !== expectedLine
      || selection.active.character !== expectedCharacter
    ) {
      return;
    }

    const edits = [];
    const addIndentEdit = (lineNumber, indent) => {
      const lineText = document.lineAt(lineNumber).text;
      const existing = leadingWhitespace(lineText);
      if (existing !== indent) {
        edits.push({
          range: new vscode.Range(
            new vscode.Position(lineNumber, 0),
            new vscode.Position(lineNumber, existing.length),
          ),
          text: indent,
        });
      }
    };
    addIndentEdit(headerLine, headerIndent);
    if (bodyLine !== undefined) {
      addIndentEdit(bodyLine, nextIndent);
    }
    if (generatedDoShell) {
      addIndentEdit(bodyLine + 1, headerIndent);
    }
    if (edits.length === 0) {
      return;
    }

    internalEdits.add(document);
    try {
      const edited = await editor.edit(
        (builder) => {
          for (const edit of edits) {
            builder.replace(edit.range, edit.text);
          }
        },
        { undoStopBefore: false, undoStopAfter: false },
      );
      if (!edited) {
        return;
      }
    } finally {
      internalEdits.delete(document);
    }

    const cursor = bodyLine === undefined
      ? new vscode.Position(headerLine, headerIndent.length + finalContentLength)
      : new vscode.Position(bodyLine, nextIndent.length);
    editor.selection = new vscode.Selection(cursor, cursor);
  }

  async function onDidChangeTextDocument(event) {
    if (internalEdits.has(event.document)) {
      return;
    }
    const plan = observedPlan(event.document, event.contentChanges)
      ?? observedTypedEndPlan(event.document, event.contentChanges);
    if (plan) {
      pendingPlans.set(event.document, plan);
    }
  }

  async function onDidChangeTextEditorSelection(event) {
    const plan = pendingPlans.get(event.textEditor.document);
    pendingPlans.delete(event.textEditor.document);
    if (plan && plan.editor === event.textEditor) {
      await applyPlan(plan);
    }
  }

  return {
    clear() {
      pendingPlans = new WeakMap();
    },
    onDidChangeTextDocument,
    onDidChangeTextEditorSelection,
  };
}

module.exports = {
  createDoEndTypingController,
  createSimiParser,
  createStructuralIndentTypingController,
  lineEndsWithCodeDo,
  parsedArmTarget,
  parsedFinalEndTarget,
};
