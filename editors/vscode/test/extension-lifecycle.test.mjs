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

function harness(plans) {
  const commands = new Map();
  const errors = [];
  const clients = [];
  const watchers = [];
  let configurationListener;
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
    environment: { SIMI_LSP_PATH: "/env/simi-lsp" },
  });
  const context = { subscriptions: [] };

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
  };
}

test("activation remains successful when simi-lsp cannot start", async () => {
  const app = harness([{ startError: "ENOENT" }]);

  await app.activate(app.context);

  assert.equal(app.clients.length, 1);
  assert.equal(app.clients[0].serverOptions.command, "/env/simi-lsp");
  assert.equal(app.watchers[0].disposed, true);
  assert.equal(app.clients[0].disposals, 1);
  assert.match(app.errors[0], /Unable to start simi-lsp/);
  assert.match(app.errors[0], /simi\.languageServer\.path/);
  assert.ok(app.commands.has("simi.restartLanguageServer"));
  assert.equal(app.context.subscriptions.length, 2);
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

  app.configure("/configured/simi-lsp");
  await app.fireConfigurationChange();

  assert.equal(app.clients[0].stops, 1);
  assert.equal(app.clients[1].serverOptions.command, "/configured/simi-lsp");
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
  assert.match(app.errors.at(-1), /Unable to stop simi-lsp/);

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
  assert.match(app.errors[0], /Unable to dispose simi-lsp/);
  assert.match(app.errors[1], /Unable to stop simi-lsp/);
});

test("stop failure is reported, cleaned, and does not launch a competing client", async () => {
  const app = harness([{ stopError: "stuck process" }, {}]);
  await app.activate(app.context);

  await app.commands.get("simi.restartLanguageServer")();

  assert.equal(app.clients.length, 1);
  assert.equal(app.watchers[0].disposed, true);
  assert.equal(app.clients[0].disposals, 1);
  assert.match(app.errors.at(-1), /Unable to stop simi-lsp/);
  await app.deactivate();
});
