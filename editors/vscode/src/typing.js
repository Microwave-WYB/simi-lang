"use strict";

const WORD_CHARACTER = /[A-Za-z0-9_]/;

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

module.exports = {
  createDoEndTypingController,
  lineEndsWithCodeDo,
};
