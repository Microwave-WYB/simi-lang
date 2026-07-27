import assert from "node:assert/strict";
import { createRequire } from "node:module";
import test from "node:test";

const require = createRequire(import.meta.url);
const { createDoEndTypingController } = require("../src/typing.js");

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
  }

  get selection() {
    return this.selections[0];
  }

  set selection(selection) {
    this.selections = [selection];
  }

  async edit(callback) {
    const edits = [];
    callback({
      delete(range) {
        edits.push({ range, text: "" });
      },
      replace(range, text) {
        edits.push({ range, text });
      },
    });
    this.editCount += 1;
    const event = this.document.applyEdits(edits);
    await this.document.changeListener(event);
    return true;
  }
}

function harness(source, cursor = undefined) {
  const document = new MockDocument(source);
  const initialCursor = cursor ?? document.positionAt(source.length);
  const editor = new MockEditor(document, initialCursor);
  const vscode = {
    Position,
    Range,
    Selection,
    window: { activeTextEditor: editor },
  };
  const controller = createDoEndTypingController({ vscode });
  document.changeListener = controller.onDidChangeTextDocument;

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
    changeDocument,
    changeSelection,
    async insert(text) {
      const next = await changeDocument(text);
      await changeSelection(next);
    },
    moveTo(line, character) {
      const position = new Position(line, character);
      editor.selection = new Selection(position, position);
    },
  };
}

test("Enter waits for VS Code's change-before-selection event sequence", async () => {
  const app = harness("  let value = do");

  const postEnterCursor = await app.changeDocument("\n");

  assert.equal(app.document.text, "  let value = do\n");
  assert.deepEqual(app.editor.selection.active, new Position(0, 16));
  assert.equal(app.editor.editCount, 0, "document change alone must only create a pending plan");

  await app.changeSelection(postEnterCursor);

  assert.equal(app.document.text, "  let value = do\n    \n  end");
  assert.deepEqual(app.editor.selection.active, new Position(1, 4));
  assert.equal(app.editor.editCount, 1, "the generated edit must not recursively add a shell");
});

test("an observed auto-indented Enter replaces editor indentation without duplication", async () => {
  const app = harness("do");

  await app.insert("\n  ");

  assert.equal(app.document.text, "do\n  \nend");
  assert.deepEqual(app.editor.selection.active, new Position(1, 2));
  assert.equal(app.editor.editCount, 1);
});

test("observed typing at a tracked generated closer replaces it without duplication", async () => {
  const app = harness("do");
  await app.insert("\n");
  await app.insert("work()");
  app.moveTo(2, 0);

  await app.insert("e");
  await app.insert("n");
  await app.insert("d");

  assert.equal(app.document.text, "do\n  work()\nend");
  assert.deepEqual(app.editor.selection.active, new Position(2, 3));
  assert.equal(app.editor.editCount, 4, "one shell edit and three closer-consumption edits");
});

test("observed Enter does not pair identifier suffixes, comments, or strings", async () => {
  for (const source of ["todo", "-- do", "\"do", "print(\"do\")"]) {
    const app = harness(source);

    await app.insert("\n");

    assert.equal(app.document.text, `${source}\n`, source);
    assert.equal(app.editor.editCount, 0, source);
  }
});

test("a nonmatching selection event cancels a pending Enter plan", async () => {
  const app = harness("do");
  await app.changeDocument("\n");

  await app.changeSelection(new Position(0, 0));
  await app.changeSelection(new Position(1, 0));

  assert.equal(app.document.text, "do\n");
  assert.equal(app.editor.editCount, 0);
});

test("multi-cursor Enter remains unmodified", async () => {
  const app = harness("do");
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
  app.editor.selection = new Selection(new Position(1, 0), new Position(1, 0));
  await app.controller.onDidChangeTextEditorSelection({
    textEditor: app.editor,
    selections: app.editor.selections,
  });

  assert.equal(app.document.text, "do\n");
  assert.equal(app.editor.editCount, 0);
});

test("ordinary changes and untracked end text are not intercepted", async () => {
  const ordinary = harness("value");
  await ordinary.insert("!");
  assert.equal(ordinary.document.text, "value!");
  assert.equal(ordinary.editor.editCount, 0);

  const existingCloser = harness("end", new Position(0, 0));
  await existingCloser.insert("e");
  assert.equal(existingCloser.document.text, "eend");
  assert.equal(existingCloser.editor.editCount, 0);
});
