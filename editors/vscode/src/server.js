"use strict";

const fs = require("node:fs");
const path = require("node:path");

function nonempty(value) {
  return typeof value === "string" && value.trim() !== "" ? value.trim() : undefined;
}

function resolveServerCommand(
  configuredPath,
  environment = process.env,
  extensionPath,
  platform = process.platform,
  exists = fs.existsSync,
) {
  const bundled = extensionPath
    ? path.join(extensionPath, "bin", platform === "win32" ? "simi.exe" : "simi")
    : undefined;
  return nonempty(configuredPath)
    ?? nonempty(environment.SIMI_PATH)
    ?? (bundled && exists(bundled) ? bundled : undefined)
    ?? "simi";
}

module.exports = { resolveServerCommand };
