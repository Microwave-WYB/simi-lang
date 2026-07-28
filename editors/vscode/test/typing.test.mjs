import assert from "node:assert/strict";
import { createRequire } from "node:module";
import test from "node:test";

const require = createRequire(import.meta.url);
const {
  createDoEndTypingController,
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
      delete(range) {
        edits.push({ range, text: "" });
      },
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
  const controllers = [controller];
  document.changeListener = async (event) => {
    for (const current of controllers) {
      await current.onDidChangeTextDocument(event);
    }
  };

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
    const event = {
      textEditor: editor,
      selections: editor.selections,
    };
    for (const current of controllers) {
      await current.onDidChangeTextEditorSelection(event);
    }
  }

  return {
    controller,
    document,
    vscode,
    addController(nextController) {
      controllers.push(nextController);
    },
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

async function structuralHarness(source, cursor = undefined, options = {}) {
  const app = harness(source, cursor);
  Object.assign(app.editor.options, options);
  const parser = await createSimiParser(new URL("../", import.meta.url).pathname);
  const structural = createStructuralIndentTypingController({
    vscode: app.vscode,
    parser,
  });
  app.addController(structural);
  return { ...app, parser, structural };
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

test("parser-backed typing indents successive direct case arms and the final end", async () => {
  const app = await structuralHarness("case value of\nfirst =>");

  await app.insert("\n");
  await app.insert("first()");
  await app.insert("\n");
  await app.insert("second =>");
  await app.insert("\n");
  await app.insert("second()");
  await app.insert("\n");
  await app.insert("end");
  assert.equal(app.document.lineAt(5).text, "end", "the final end must align before another Enter");
  await app.insert("\n");

  assert.equal(app.document.text, [
    "case value of",
    "  first =>",
    "    first()",
    "  second =>",
    "    second()",
    "end",
    "",
  ].join("\n"));
  assert.deepEqual(indentationLevels(app.document.text), [0, 1, 2, 1, 2, 0, 0]);
  assert.ok(
    app.editor.editOptions.every(
      (options) => options.undoStopBefore === false && options.undoStopAfter === false,
    ),
    "structural indentation edits must join the Enter undo group",
  );
  app.parser.delete();
});

test("parser-backed typing indents successive catch arms", async () => {
  const app = await structuralHarness([
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
  assert.equal(app.document.lineAt(7).text, "end", "the protected final end must align immediately");
  await app.insert("\n");

  assert.deepEqual(indentationLevels(app.document.text), [0, 1, 0, 1, 2, 1, 2, 0, 0]);
  app.parser.delete();
});

test("parser-backed typing preserves the generated do shell used as a direct arm expression", async () => {
  const app = await structuralHarness("case value of\n_ => do");

  await app.insert("\n");

  assert.equal(app.document.text, "case value of\n  _ => do\n    \n  end");
  assert.deepEqual(app.editor.selection.active, new Position(2, 4));
  assert.equal(app.editor.editCount, 2, "the do shell and structural indent are separate joined edits");
  app.parser.delete();
});

test("parser-backed typing treats a multiline do block as an ordinary direct arm expression", async () => {
  const app = await structuralHarness("case value of\n_ =>");

  await app.insert("\n");
  await app.insert("do");
  await app.insert("\n");

  assert.equal(app.document.text, [
    "case value of",
    "  _ =>",
    "    do",
    "      ",
    "    end",
  ].join("\n"));
  assert.deepEqual(app.editor.selection.active, new Position(3, 6));
  app.parser.delete();
});

test("parser-backed typing leaves complete same-line arm expressions unchanged", async () => {
  for (const arm of ["first => first()", "_ => do value end"]) {
    const source = `case value of\n${arm}`;
    const app = await structuralHarness(source);

    await app.insert("\n");

    assert.equal(app.document.text, `${source}\n`);
    assert.equal(app.editor.editCount, 0);
    app.parser.delete();
  }
});

test("parser-backed typing uses the nearest nested case owner", async () => {
  const app = await structuralHarness([
    "case outer of",
    "  _ =>",
    "    case inner of",
    "first =>",
  ].join("\n"));

  await app.insert("\n");

  assert.equal(app.document.lineAt(3).text, "      first =>");
  assert.equal(app.document.lineAt(4).text, "        ");
  app.parser.delete();
});

test("parser-backed typing has no fixed nesting-depth completion cap", async () => {
  const lines = [];
  const depth = 12;
  for (let level = 0; level < depth; level += 1) {
    lines.push(`${"  ".repeat(level * 2)}case ${level} of`);
    if (level < depth - 1) {
      lines.push(`${"  ".repeat(level * 2 + 1)}_ =>`);
    }
  }
  lines.push("target =>");
  const app = await structuralHarness(lines.join("\n"));

  await app.insert("\n");

  assert.equal(app.document.lineAt(depth * 2 - 1).text, `${"  ".repeat(depth * 2 - 1)}target =>`);
  assert.equal(app.document.lineAt(depth * 2).text, "  ".repeat(depth * 2));
  app.parser.delete();
});

test("parser-backed typing fails open when a parse exceeds its budget", () => {
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
  assert.equal(parser.resets, 3, "each canceled candidate parse must reset parser state");
});

test("parser-backed typing respects hard tabs and non-default tab settings", async () => {
  const app = await structuralHarness("\tcase value of\n_ =>", undefined, {
    insertSpaces: false,
    tabSize: 8,
  });

  await app.insert("\n");

  assert.equal(app.document.text, "\tcase value of\n\t\t_ =>\n\t\t\t");
  app.parser.delete();
});

test("parser-backed typing ignores comments, strings, invalid syntax, and one-line forms", async () => {
  for (const source of [
    "case value of\n-- fake =>",
    "case value of\n\"fake =>\"",
    "case value of\n) =>",
    "case value of _ => do value end",
  ]) {
    const app = await structuralHarness(source);

    await app.insert("\n");

    assert.equal(app.document.text, `${source}\n`, source);
    assert.equal(app.editor.editCount, 0, source);
    app.parser.delete();
  }
});

test("parser-backed typing leaves multi-cursor documents unmodified", async () => {
  const app = await structuralHarness("case value of\nfirst =>");
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
