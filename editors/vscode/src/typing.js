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

  function eligibleEnter(editor, text) {
    if (text !== "\n" && text !== "\r\n") {
      return undefined;
    }
    if (editor.document.languageId !== "simi" || editor.selections.length !== 1) {
      return undefined;
    }
    const selection = editor.selection;
    if (!selection.isEmpty) {
      return undefined;
    }
    const line = editor.document.lineAt(selection.active.line);
    if (!lineEndsWithCodeDo(line.text, selection.active.character)) {
      return undefined;
    }
    const baseIndent = leadingWhitespace(line.text);
    return {
      document: editor.document,
      editor,
      line: selection.active.line,
      baseIndent,
      childIndent: baseIndent + indentationUnit(editor.options),
    };
  }

  function eligibleOvertype(editor, text) {
    if (editor.document.languageId !== "simi" || editor.selections.length !== 1) {
      return undefined;
    }
    if (text.length === 0 || text.includes("\n") || text.includes("\r")) {
      return undefined;
    }
    const selection = editor.selection;
    if (!selection.isEmpty) {
      return undefined;
    }
    const offset = editor.document.offsetAt(selection.active);
    const marker = closers.at(editor.document, offset);
    if (!marker || offset + text.length > marker.end) {
      return undefined;
    }
    const range = new vscode.Range(
      editor.document.positionAt(offset),
      editor.document.positionAt(offset + text.length),
    );
    if (editor.document.getText(range) !== text) {
      return undefined;
    }
    return { document: editor.document, editor, marker, length: text.length, text };
  }

  async function consumeGeneratedCloser(plan) {
    const { document, editor, marker, length, text } = plan;
    if (vscode.window.activeTextEditor !== editor || !closers.contains(document, marker)) {
      return;
    }
    const originalStart = marker.start;
    const originalEnd = originalStart + length;
    const range = new vscode.Range(
      document.positionAt(originalStart),
      document.positionAt(originalEnd),
    );
    if (document.getText(range) !== text) {
      closers.remove(document, marker);
      return;
    }

    closers.prepareConsumption(marker, length);
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
  }

  async function insertBlockShell(plan) {
    const { document, editor, line, baseIndent, childIndent } = plan;
    if (vscode.window.activeTextEditor !== editor || editor.document !== document) {
      return;
    }
    const selection = editor.selection;
    if (
      editor.selections.length !== 1
      || !selection.isEmpty
      || selection.active.line !== line + 1
    ) {
      return;
    }

    const bodyLine = document.lineAt(selection.active.line);
    const bodyPrefix = bodyLine.text.slice(0, selection.active.character);
    if (!/^[ \t]*$/.test(bodyPrefix)) {
      return;
    }

    const bodyStart = new vscode.Position(selection.active.line, 0);
    const replaceRange = new vscode.Range(bodyStart, selection.active);
    const replacement = `${childIndent}\n${baseIndent}end`;
    const edited = await editor.edit(
      (builder) => builder.replace(replaceRange, replacement),
      { undoStopBefore: false, undoStopAfter: false },
    );
    if (!edited) {
      return;
    }

    const bodyCursor = new vscode.Position(bodyStart.line, childIndent.length);
    editor.selection = new vscode.Selection(bodyCursor, bodyCursor);
    const closeStart = new vscode.Position(bodyStart.line + 1, baseIndent.length);
    closers.add(document, document.offsetAt(closeStart), document.offsetAt(closeStart) + 3);
  }

  async function type(args) {
    const text = args && typeof args.text === "string" ? args.text : undefined;
    const editor = vscode.window.activeTextEditor;
    const enterPlan = editor && text !== undefined ? eligibleEnter(editor, text) : undefined;
    const overtypePlan = editor && text !== undefined ? eligibleOvertype(editor, text) : undefined;

    await vscode.commands.executeCommand("default:type", args);

    if (overtypePlan) {
      await consumeGeneratedCloser(overtypePlan);
    } else if (enterPlan) {
      await insertBlockShell(enterPlan);
    }
  }

  function onDidChangeTextDocument(event) {
    closers.applyChanges(event.document, event.contentChanges);
  }

  return {
    clear: () => closers.clear(),
    onDidChangeTextDocument,
    type,
  };
}

module.exports = {
  createDoEndTypingController,
  lineEndsWithCodeDo,
};
