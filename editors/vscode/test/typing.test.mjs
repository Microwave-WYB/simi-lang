import assert from "node:assert/strict";
import { createRequire } from "node:module";
import test from "node:test";

const require = createRequire(import.meta.url);
const {
  createSimiParser,
  createStructuralIndentTypingController,
  parsedArmTarget,
} = require("../src/typing.js");

class Position {
  constructor(line, character) {
    this.line = line;
    this.character = character;
  }
}

class Range {
  constructor(start, end) {
    this.start = start;
    this.end = end;
  }
}

class Selection extends Range {
  constructor(anchor, active) {
    super(anchor, active);
    this.anchor = anchor;
    this.active = active;
  }

  get isEmpty() {
    return this.anchor.line === this.active.line
      && this.anchor.character === this.active.character;
  }
}

class MockDocument {
  constructor(text) {
    this.text = text;
    this.languageId = "simi";
    this.changeListener = async () => {};
  }

  lineAt(line) {
    return { text: this.text.split("\n")[line] ?? "" };
  }

  offsetAt(position) {
    const lines = this.text.split("\n");
    let offset = 0;
    for (let line = 0; line < position.line; line += 1) {
      offset += lines[line].length + 1;
    }
    return offset + position.character;
  }

  positionAt(requestedOffset) {
    let offset = Math.max(0, Math.min(requestedOffset, this.text.length));
    const lines = this.text.split("\n");
    for (let line = 0; line < lines.length; line += 1) {
      if (offset <= lines[line].length) {
        return new Position(line, offset);
      }
      offset -= lines[line].length + 1;
    }
    return new Position(lines.length - 1, lines.at(-1).length);
  }

  getText(range) {
    if (!range) {
      return this.text;
    }
    return this.text.slice(this.offsetAt(range.start), this.offsetAt(range.end));
  }

  applyEdits(edits) {
    const changes = edits.map(({ range, text }) => ({
      range,
      rangeOffset: this.offsetAt(range.start),
      rangeLength: this.offsetAt(range.end) - this.offsetAt(range.start),
      text,
    }));
    for (const change of [...changes].sort((left, right) => right.rangeOffset - left.rangeOffset)) {
      this.text = this.text.slice(0, change.rangeOffset)
        + change.text
        + this.text.slice(change.rangeOffset + change.rangeLength);
    }
    return { document: this, contentChanges: changes };
  }
}

class MockEditor {
  constructor(document, cursor) {
    this.document = document;
    this.options = { insertSpaces: true, tabSize: 2 };
    this.selection = new Selection(cursor, cursor);
    this.editCount = 0;
    this.editOptions = [];
  }

  get selection() {
    return this.selections[0];
  }

  set selection(selection) {
    this.selections = [selection];
  }

  async edit(callback, options) {
    const edits = [];
    callback({
      replace(range, text) {
        edits.push({ range, text });
      },
    });
    this.editCount += 1;
    this.editOptions.push(options);
    const event = this.document.applyEdits(edits);
    await this.document.changeListener(event);
    return true;
  }
}

async function harness(source, cursor = undefined, options = {}) {
  const document = new MockDocument(source);
  const initialCursor = cursor ?? document.positionAt(source.length);
  const editor = new MockEditor(document, initialCursor);
  Object.assign(editor.options, options);
  const vscode = {
    Position,
    Range,
    Selection,
    window: { activeTextEditor: editor },
  };
  const parser = await createSimiParser(new URL("../", import.meta.url).pathname);
  const controller = createStructuralIndentTypingController({ vscode, parser });
  document.changeListener = (event) => controller.onDidChangeTextDocument(event);

  async function changeDocument(text) {
    const selection = editor.selection;
    const start = document.offsetAt(selection.start);
    const range = new Range(selection.start, selection.end);
    const event = document.applyEdits([{ range, text }]);
    const next = document.positionAt(start + text.length);
    await document.changeListener(event);
    return next;
  }

  async function changeSelection(position) {
    editor.selection = new Selection(position, position);
    await controller.onDidChangeTextEditorSelection({
      textEditor: editor,
      selections: editor.selections,
    });
  }

  return {
    controller,
    document,
    editor,
    parser,
    async insert(text) {
      const next = await changeDocument(text);
      await changeSelection(next);
    },
  };
}

function indentationLevels(source, tabSize = 2) {
  return source.split("\n").map((line) => {
    const prefix = line.match(/^[ \t]*/)[0];
    return [...prefix].reduce(
      (width, character) => width + (character === "\t" ? 1 : 1 / tabSize),
      0,
    );
  });
}

test("parser-backed typing restores case sibling arm, body, and end levels", async () => {
  const app = await harness("case value of\nfirst =>");

  await app.insert("\n");
  await app.insert("first()");
  await app.insert("\n");
  await app.insert("second =>");
  await app.insert("\n");
  await app.insert("second()");
  await app.insert("\n");
  await app.insert("end");

  assert.equal(app.document.text, [
    "case value of",
    "  first =>",
    "    first()",
    "  second =>",
    "    second()",
    "end",
  ].join("\n"));
  assert.deepEqual(indentationLevels(app.document.text), [0, 1, 2, 1, 2, 0]);
  assert.ok(
    app.editor.editOptions.every(
      (options) => options.undoStopBefore === false && options.undoStopAfter === false,
    ),
    "structural edits must remain in VS Code's typing undo group",
  );
  app.parser.delete();
});

test("parser-backed typing restores catch sibling arm, body, and end levels", async () => {
  const app = await harness([
    "do",
    "  prepare()",
    "catch of",
    "first =>",
  ].join("\n"));

  await app.insert("\n");
  await app.insert("recover_first()");
  await app.insert("\n");
  await app.insert("second =>");
  await app.insert("\n");
  await app.insert("recover_second()");
  await app.insert("\n");
  await app.insert("end");

  assert.deepEqual(indentationLevels(app.document.text), [0, 1, 0, 1, 2, 1, 2, 0]);
  assert.deepEqual(
    indentationLevels(app.document.text).slice(2),
    [0, 1, 2, 1, 2, 0],
    "catch header, sibling arms, bodies, and final end must match the structural target",
  );
  app.parser.delete();
});

test("structural typing indents an explicit do arm without creating a shell", async () => {
  const app = await harness("case value of\n_ => do");

  await app.insert("\n");

  assert.equal(app.document.text, "case value of\n  _ => do\n    ");
  assert.equal(app.document.text.split("\n").filter((line) => line.trim() === "end").length, 0);
  assert.deepEqual(app.editor.selection.active, new Position(2, 4));
  app.parser.delete();
});

test("standalone keywords and headers never create shells or closing ends", async () => {
  for (const source of ["do", "case", "case value of", "fn", "fn named() do"]) {
    const app = await harness(source);

    await app.insert("\n");

    assert.equal(app.document.text, `${source}\n`, source);
    assert.equal(app.editor.editCount, 0, source);
    assert.equal(app.document.text.includes("end"), false, source);
    app.parser.delete();
  }
});

test("parser-backed typing uses the nearest owner and configured hard tabs", async () => {
  const app = await harness([
    "\tcase outer of",
    "\t\t_ =>",
    "\t\t\tcase inner of",
    "target =>",
  ].join("\n"), undefined, {
    insertSpaces: false,
    tabSize: 8,
  });

  await app.insert("\n");

  assert.equal(app.document.lineAt(3).text, "\t\t\t\ttarget =>");
  assert.equal(app.document.lineAt(4).text, "\t\t\t\t\t");
  app.parser.delete();
});

test("parser-backed typing ignores comments, strings, invalid syntax, and multi-cursor input", async () => {
  for (const source of [
    "case value of\n-- fake =>",
    "case value of\n\"fake =>\"",
    "case value of\n) =>",
    "case value of _ => do value end",
  ]) {
    const app = await harness(source);

    await app.insert("\n");

    assert.equal(app.document.text, `${source}\n`, source);
    assert.equal(app.editor.editCount, 0, source);
    app.parser.delete();
  }

  const app = await harness("case value of\nfirst =>");
  const cursor = app.editor.selection.active;
  app.editor.selections = [
    new Selection(cursor, cursor),
    new Selection(cursor, cursor),
  ];
  const event = app.document.applyEdits([{
    range: new Range(cursor, cursor),
    text: "\n",
  }]);
  await app.document.changeListener(event);

  assert.equal(app.document.text, "case value of\nfirst =>\n");
  assert.equal(app.editor.editCount, 0);
  app.parser.delete();
});

test("parser cancellation resets state and fails open", () => {
  const parser = {
    resets: 0,
    parse(_source, _oldTree, options) {
      assert.equal(typeof options.progressCallback, "function");
      return null;
    },
    reset() {
      this.resets += 1;
    },
  };

  assert.equal(parsedArmTarget(parser, "case value of\n_ =>\n", 1, 20), undefined);
  assert.equal(parser.resets, 3);
});
