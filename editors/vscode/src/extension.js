"use strict";

const vscode = require("vscode");
const {
  CloseAction,
  ErrorAction,
  LanguageClient,
} = require("vscode-languageclient/node");
const { resolveServerCommand } = require("./server");

let client;
let starting;

function serverOptions() {
  const configuredPath = vscode.workspace
    .getConfiguration("simi")
    .get("languageServer.path");
  return {
    command: resolveServerCommand(configuredPath),
    options: { env: process.env },
  };
}

function clientOptions() {
  return {
    documentSelector: [{ language: "simi", scheme: "file" }],
    synchronize: {
      configurationSection: "simi",
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.simi"),
    },
    errorHandler: {
      error() {
        return { action: ErrorAction.Continue };
      },
      closed() {
        return { action: CloseAction.Restart };
      },
    },
  };
}

async function startClient(context) {
  if (client || starting) {
    return starting;
  }
  const next = new LanguageClient(
    "simi-lsp",
    "Simi Language Server",
    serverOptions(),
    clientOptions(),
  );
  client = next;
  starting = next.start().catch((error) => {
    if (client === next) {
      client = undefined;
    }
    void vscode.window.showErrorMessage(
      `Unable to start simi-lsp: ${error instanceof Error ? error.message : String(error)}`,
    );
    throw error;
  }).finally(() => {
    starting = undefined;
  });
  context.subscriptions.push(next);
  return starting;
}

async function stopClient() {
  const active = client;
  client = undefined;
  if (active) {
    await active.stop();
  }
}

async function restartClient(context) {
  if (starting) {
    try {
      await starting;
    } catch {
      // Startup already reported the failure; retry with freshly resolved options.
    }
  }
  await stopClient();
  await startClient(context);
}

async function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand("simi.restartLanguageServer", () =>
      restartClient(context),
    ),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("simi.languageServer.path")) {
        void restartClient(context);
      }
    }),
  );
  await startClient(context);
}

async function deactivate() {
  await stopClient();
}

module.exports = { activate, deactivate };
