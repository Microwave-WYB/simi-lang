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
    this.changeListener = () => {};
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
      rangeOffset: this.offsetAt(range.start),
      rangeLength: this.offsetAt(range.end) - this.offsetAt(range.start),
      text,
    }));
    for (const change of [...changes].sort((left, right) => right.rangeOffset - left.rangeOffset)) {
      this.text = this.text.slice(0, change.rangeOffset)
        + change.text
        + this.text.slice(change.rangeOffset + change.rangeLength);
    }
    this.changeListener({ document: this, contentChanges: changes });
  }
}

class MockEditor {
  constructor(document, cursor) {
    this.document = document;
    this.options = { insertSpaces: true, tabSize: 2 };
    this.selection = new Selection(cursor, cursor);
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
    this.document.applyEdits(edits);
    return true;
  }
}

function harness(source, cursor = undefined) {
  const document = new MockDocument(source);
  const initialCursor = cursor ?? document.positionAt(source.length);
  const editor = new MockEditor(document, initialCursor);
  const defaultTyped = [];
  const vscode = {
    Position,
    Range,
    Selection,
    window: { activeTextEditor: editor },
    commands: {
      async executeCommand(command, args) {
        assert.equal(command, "default:type");
        defaultTyped.push(args.text);
        const selection = editor.selection;
        const start = document.offsetAt(selection.start);
        const range = new Range(selection.start, selection.end);
        document.applyEdits([{ range, text: args.text }]);
        const next = document.positionAt(start + args.text.length);
        editor.selection = new Selection(next, next);
      },
    },
  };
  const controller = createDoEndTypingController({ vscode });
  document.changeListener = controller.onDidChangeTextDocument;

  return {
    controller,
    defaultTyped,
    document,
    editor,
    async type(text) {
      await controller.type({ text });
    },
    moveTo(line, character) {
      const position = new Position(line, character);
      editor.selection = new Selection(position, position);
    },
  };
}

test("Enter after a code do inserts an indented shell through the type command", async () => {
  const app = harness("  let value = do");

  await app.type("\n");

  assert.equal(app.document.text, "  let value = do\n    \n  end");
  assert.deepEqual(app.editor.selection.active, new Position(1, 4));
  assert.deepEqual(app.defaultTyped, ["\n"], "Enter must pass through default:type first");
});

test("typing end at the tracked generated closer replaces it without duplication", async () => {
  const app = harness("do");
  await app.type("\n");
  await app.type("work()");
  app.moveTo(2, 0);

  await app.type("e");
  await app.type("n");
  await app.type("d");

  assert.equal(app.document.text, "do\n  work()\nend");
  assert.deepEqual(app.editor.selection.active, new Position(2, 3));
  assert.deepEqual(app.defaultTyped, ["\n", "work()", "e", "n", "d"]);
});

test("typing do does not pair early while an identifier may still be forming", async () => {
  const app = harness("d");

  await app.type("o");
  await app.type("c");
  await app.type("u");

  assert.equal(app.document.text, "docu");
  assert.deepEqual(app.defaultTyped, ["o", "c", "u"]);
});

test("Enter does not pair identifier suffixes, comments, or strings", async () => {
  for (const source of ["todo", "-- do", "\"do", "print(\"do\")"]) {
    const app = harness(source);

    await app.type("\n");

    assert.equal(app.document.text, `${source}\n`, source);
    assert.deepEqual(app.defaultTyped, ["\n"]);
  }
});

test("ordinary typing and untracked end text retain default:type behavior", async () => {
  const ordinary = harness("value");
  await ordinary.type("!");
  assert.equal(ordinary.document.text, "value!");

  const existingCloser = harness("end", new Position(0, 0));
  await existingCloser.type("e");
  assert.equal(existingCloser.document.text, "eend");
  assert.deepEqual(existingCloser.defaultTyped, ["e"]);
});
