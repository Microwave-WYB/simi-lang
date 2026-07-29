"use strict";

const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");
const { createExtensionRuntime } = require("./extension-runtime");
const { resolveServerCommand } = require("./server");
const { createSimiParser } = require("./typing");

const runtime = createExtensionRuntime({
  vscode,
  LanguageClient,
  resolveServerCommand,
  environment: process.env,
  createSimiParser,
});

module.exports = {
  activate: runtime.activate,
  deactivate: runtime.deactivate,
};
