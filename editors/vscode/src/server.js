"use strict";

function nonempty(value) {
  return typeof value === "string" && value.trim() !== "" ? value.trim() : undefined;
}

function resolveServerCommand(configuredPath, environment = process.env) {
  return nonempty(configuredPath) ?? nonempty(environment.SIMI_PATH) ?? "simi";
}

module.exports = { resolveServerCommand };
