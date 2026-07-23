"use strict";

function createExtensionRuntime({ vscode, LanguageClient, resolveServerCommand, environment }) {
  let active;
  let starting;
  let restartQueue = Promise.resolve();
  let deactivated = false;

  function reportFailure(action, command, error) {
    const detail = error instanceof Error ? error.message : String(error);
    const message = `${action} simi-lsp (${command}): ${detail}. Configure simi.languageServer.path, set SIMI_LSP_PATH, or install simi-lsp on PATH.`;
    return Promise.resolve(vscode.window.showErrorMessage(message)).catch(() => undefined);
  }

  function serverOptions() {
    const configuredPath = vscode.workspace
      .getConfiguration("simi")
      .get("languageServer.path");
    return {
      command: resolveServerCommand(configuredPath, environment),
      options: { env: environment },
    };
  }

  function clientOptions(watcher) {
    return {
      documentSelector: [{ language: "simi", scheme: "file" }],
      synchronize: {
        configurationSection: "simi",
        fileEvents: watcher,
      },
    };
  }

  async function disposeAfterFailure(current, command) {
    if (typeof current.dispose !== "function") {
      return;
    }
    try {
      await current.dispose();
    } catch (error) {
      await reportFailure("Unable to dispose", command, error);
    }
  }

  async function stopActive() {
    if (starting) {
      await starting;
    }
    const current = active;
    active = undefined;
    if (!current) {
      return true;
    }
    try {
      await current.client.stop();
      return true;
    } catch (error) {
      await disposeAfterFailure(current.client, current.command);
      await reportFailure("Unable to stop", current.command, error);
      return false;
    } finally {
      current.watcher.dispose();
    }
  }

  function startClient() {
    if (deactivated || active) {
      return Promise.resolve(Boolean(active));
    }
    if (starting) {
      return starting;
    }

    const options = serverOptions();
    const watcher = vscode.workspace.createFileSystemWatcher("**/*.simi");
    const next = new LanguageClient(
      "simi-lsp",
      "Simi Language Server",
      options,
      clientOptions(watcher),
    );

    starting = (async () => {
      try {
        await next.start();
        if (deactivated) {
          try {
            await next.stop();
          } catch (error) {
            await disposeAfterFailure(next, options.command);
            await reportFailure("Unable to stop", options.command, error);
          } finally {
            watcher.dispose();
          }
          return false;
        }
        active = { client: next, watcher, command: options.command };
        return true;
      } catch (error) {
        watcher.dispose();
        await disposeAfterFailure(next, options.command);
        await reportFailure("Unable to start", options.command, error);
        return false;
      } finally {
        starting = undefined;
      }
    })();
    return starting;
  }

  async function restartClient() {
    if (deactivated) {
      return false;
    }
    const stopped = await stopActive();
    if (!stopped || deactivated) {
      return false;
    }
    return startClient();
  }

  function queueRestart() {
    const operation = restartQueue.then(restartClient, restartClient);
    restartQueue = operation.catch(async (error) => {
      await reportFailure("Unable to restart", serverOptions().command, error);
      return false;
    });
    return restartQueue;
  }

  async function activate(context) {
    deactivated = false;
    context.subscriptions.push(
      vscode.commands.registerCommand("simi.restartLanguageServer", queueRestart),
      vscode.workspace.onDidChangeConfiguration((event) => {
        if (event.affectsConfiguration("simi.languageServer.path")) {
          return queueRestart();
        }
        return Promise.resolve(false);
      }),
    );
    await startClient();
  }

  async function deactivate() {
    deactivated = true;
    await restartQueue;
    await stopActive();
  }

  return { activate, deactivate };
}

module.exports = { createExtensionRuntime };
