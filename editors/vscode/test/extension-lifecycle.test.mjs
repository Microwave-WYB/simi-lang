import assert from "node:assert/strict";
import { createRequire } from "node:module";
import test from "node:test";

const require = createRequire(import.meta.url);
const { createExtensionRuntime } = require("../src/extension-runtime.js");
const { resolveServerCommand } = require("../src/server.js");

function deferred() {
  let resolve;
  const promise = new Promise((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
}

function harness(plans, createSimiParser = undefined) {
  const commands = new Map();
  const errors = [];
  const clients = [];
  const watchers = [];
  let configurationListener;
  let textDocumentListener;
  let textEditorSelectionListener;
  let configuredPath = "";

  const vscode = {
    commands: {
      registerCommand(name, callback) {
        commands.set(name, callback);
        return { dispose() {} };
      },
    },
    window: {
      async showErrorMessage(message) {
        errors.push(message);
      },
      onDidChangeTextEditorSelection(callback) {
        textEditorSelectionListener = callback;
        return { dispose() {} };
      },
    },
    workspace: {
      createFileSystemWatcher(pattern) {
        const watcher = {
          pattern,
          disposed: false,
          dispose() {
            this.disposed = true;
          },
        };
        watchers.push(watcher);
        return watcher;
      },
      getConfiguration(section) {
        assert.equal(section, "simi");
        return {
          get(key) {
            assert.equal(key, "languageServer.path");
            return configuredPath;
          },
        };
      },
      onDidChangeConfiguration(callback) {
        configurationListener = callback;
        return { dispose() {} };
      },
      onDidChangeTextDocument(callback) {
        textDocumentListener = callback;
        return { dispose() {} };
      },
    },
  };

  class LanguageClient {
    constructor(id, name, serverOptions, clientOptions) {
      this.id = id;
      this.name = name;
      this.serverOptions = serverOptions;
      this.clientOptions = clientOptions;
      this.plan = plans[clients.length] ?? {};
      this.starts = 0;
      this.stops = 0;
      this.disposals = 0;
      clients.push(this);
    }

    async start() {
      this.starts += 1;
      if (this.plan.startGate) await this.plan.startGate.promise;
      if (this.plan.startError) throw new Error(this.plan.startError);
    }

    async stop() {
      this.stops += 1;
      if (this.plan.stopError) throw new Error(this.plan.stopError);
    }

    async dispose() {
      this.disposals += 1;
      if (this.plan.disposeError) throw new Error(this.plan.disposeError);
    }
  }

  const runtime = createExtensionRuntime({
    vscode,
    LanguageClient,
    resolveServerCommand,
    environment: { SIMI_PATH: "/env/simi" },
    createSimiParser,
  });
  const context = { extensionPath: "/extension", subscriptions: [] };

  return {
    ...runtime,
    clients,
    commands,
    context,
    errors,
    watchers,
    configure(path) {
      configuredPath = path;
    },
    fireConfigurationChange(affects = true) {
      return configurationListener({
        affectsConfiguration(key) {
          assert.equal(key, "simi.languageServer.path");
          return affects;
        },
      });
    },
    get textDocumentListener() {
      return textDocumentListener;
    },
    get textEditorSelectionListener() {
      return textEditorSelectionListener;
    },
  };
}

test("activation loads structural indentation without intercepting global typing", async () => {
  const parser = { deleted: false, delete() { this.deleted = true; } };
  const paths = [];
  const app = harness([{}], async (extensionPath) => {
    paths.push(extensionPath);
    return parser;
  });

  await app.activate(app.context);

  assert.deepEqual(paths, ["/extension"]);
  assert.equal(app.commands.has("type"), false, "VSCodeVim-compatible typing must stay event-based");
  assert.equal(typeof app.textDocumentListener, "function");
  assert.equal(typeof app.textEditorSelectionListener, "function");
  assert.equal(app.context.subscriptions.length, 4);

  await app.deactivate();
  assert.equal(parser.deleted, true);
});

test("activation falls back safely when the structural parser cannot load", async () => {
  const app = harness([{}], async () => {
    throw new Error("invalid parser module");
  });

  await app.activate(app.context);

  assert.equal(app.clients.length, 1);
  assert.equal(app.clients[0].starts, 1, "parser failure must not disable the language server");
  assert.deepEqual(app.errors, [
    "Unable to load the Simi indentation parser: invalid parser module",
  ]);
  assert.equal(typeof app.textDocumentListener, "function");
  assert.equal(typeof app.textEditorSelectionListener, "function");

  await app.deactivate();
});

test("activation remains successful when simi lsp cannot start", async () => {
  const app = harness([{ startError: "ENOENT" }]);

  await app.activate(app.context);

  assert.equal(app.clients.length, 1);
  assert.equal(app.clients[0].serverOptions.command, "/env/simi");
  assert.deepEqual(app.clients[0].serverOptions.args, ["lsp"]);
  assert.equal(app.watchers[0].disposed, true);
  assert.equal(app.clients[0].disposals, 1);
  assert.match(app.errors[0], /Unable to start simi lsp/);
  assert.match(app.errors[0], /simi\.languageServer\.path/);
  assert.ok(app.commands.has("simi.restartLanguageServer"));
  assert.equal(app.commands.has("type"), false, "activation must not intercept global typing");
  assert.equal(typeof app.textDocumentListener, "function");
  assert.equal(typeof app.textEditorSelectionListener, "function");
  assert.equal(app.context.subscriptions.length, 4);
});

test("successful activation uses language client defaults and deactivates cleanly", async () => {
  const app = harness([{}]);

  await app.activate(app.context);

  assert.equal(app.clients.length, 1);
  assert.equal(app.clients[0].starts, 1);
  assert.equal(app.clients[0].clientOptions.errorHandler, undefined);
  assert.equal(app.clients[0].clientOptions.synchronize.fileEvents.pattern, "**/*.simi");

  await app.deactivate();

  assert.equal(app.clients[0].stops, 1);
  assert.equal(app.watchers[0].disposed, true);
});

test("configuration restart failures are handled and leave restart command usable", async () => {
  const app = harness([{}, { startError: "permission denied" }, {}]);
  await app.activate(app.context);

  app.configure("/configured/simi");
  await app.fireConfigurationChange();

  assert.equal(app.clients[0].stops, 1);
  assert.equal(app.clients[1].serverOptions.command, "/configured/simi");
  assert.deepEqual(app.clients[1].serverOptions.args, ["lsp"]);
  assert.match(app.errors.at(-1), /permission denied/);

  await app.commands.get("simi.restartLanguageServer")();

  assert.equal(app.clients.length, 3);
  assert.equal(app.clients[2].starts, 1);
  await app.deactivate();
  assert.equal(app.clients[2].stops, 1);
});

test("manual restarts are serialized and cannot race client state", async () => {
  const app = harness([{}, {}, {}]);
  await app.activate(app.context);
  const restart = app.commands.get("simi.restartLanguageServer");

  await Promise.all([restart(), restart()]);

  assert.equal(app.clients.length, 3);
  assert.deepEqual(
    app.clients.map((client) => [client.starts, client.stops]),
    [[1, 1], [1, 1], [1, 0]],
  );
  assert.deepEqual(
    app.watchers.map((watcher) => watcher.disposed),
    [true, true, false],
  );

  await app.deactivate();
  assert.equal(app.clients[2].stops, 1);
});

test("deactivation during delayed startup disposes a client whose stop fails", async () => {
  const startGate = deferred();
  const app = harness([{ startGate, stopError: "stuck startup process" }, {}]);

  const activation = app.activate(app.context);
  assert.equal(app.clients[0].starts, 1);
  const deactivation = app.deactivate();

  startGate.resolve();
  await Promise.all([activation, deactivation]);

  assert.equal(app.clients[0].stops, 1);
  assert.equal(app.clients[0].disposals, 1);
  assert.equal(app.watchers[0].disposed, true);
  assert.match(app.errors.at(-1), /Unable to stop simi lsp/);

  await app.activate(app.context);
  assert.equal(app.clients.length, 2);
  assert.equal(app.clients[1].starts, 1);
  await app.deactivate();
  assert.equal(app.clients[1].stops, 1);
});

test("cleanup failures are reported without rejecting deactivation", async () => {
  const startGate = deferred();
  const app = harness([{
    startGate,
    stopError: "stuck startup process",
    disposeError: "dispose failed",
  }]);

  const activation = app.activate(app.context);
  const deactivation = app.deactivate();
  startGate.resolve();
  await Promise.all([activation, deactivation]);

  assert.equal(app.clients[0].disposals, 1);
  assert.equal(app.watchers[0].disposed, true);
  assert.match(app.errors[0], /Unable to dispose simi lsp/);
  assert.match(app.errors[1], /Unable to stop simi lsp/);
});

test("stop failure is reported, cleaned, and does not launch a competing client", async () => {
  const app = harness([{ stopError: "stuck process" }, {}]);
  await app.activate(app.context);

  await app.commands.get("simi.restartLanguageServer")();

  assert.equal(app.clients.length, 1);
  assert.equal(app.watchers[0].disposed, true);
  assert.equal(app.clients[0].disposals, 1);
  assert.match(app.errors.at(-1), /Unable to stop simi lsp/);
  await app.deactivate();
});
