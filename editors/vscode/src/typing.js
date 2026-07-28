"use strict";

const path = require("node:path");
const { Language, Parser } = require("web-tree-sitter");

const ENTER_INSERTION = /^(?:\r\n|\n)([ \t]*)$/;
const PARSE_BUDGET_MILLISECONDS = 20;

function leadingWhitespace(text) {
  return text.match(/^[ \t]*/)[0];
}

function indentationUnit(options) {
  const tabSize = Number.isInteger(options.tabSize) ? options.tabSize : 4;
  return options.insertSpaces === false ? "\t" : " ".repeat(tabSize);
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
      return {
        ownerLine: arm.parent.startPosition.row,
        bodyIndent: blockContinues || body.startPosition.row > headerLine,
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

  function observedEnterPlan(document, contentChanges) {
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
    const arm = /=>\s*(?:do\s*)?(?:--.*)?$/.test(line.text)
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
      headerIndent: ownerIndent,
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
      finalContentLength,
    } = plan;
    if (vscode.window.activeTextEditor !== editor || editor.document !== document) {
      return;
    }
    const selection = editor.selection;
    const expectedLine = plan.expectedLine ?? bodyLine;
    const expectedCharacter = plan.expectedCharacter ?? insertedIndent.length;
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
    const plan = observedEnterPlan(event.document, event.contentChanges)
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
  createSimiParser,
  createStructuralIndentTypingController,
  parsedArmTarget,
  parsedFinalEndTarget,
};
