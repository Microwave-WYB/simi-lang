"use strict";

const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");
const { createExtensionRuntime } = require("./extension-runtime");
const { resolveServerCommand } = require("./server");

const runtime = createExtensionRuntime({
  vscode,
  LanguageClient,
  resolveServerCommand,
  environment: process.env,
});

module.exports = {
  activate: runtime.activate,
  deactivate: runtime.deactivate,
};
