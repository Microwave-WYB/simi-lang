import { chmodSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const grammarPath = fileURLToPath(new URL("../../tree-sitter", import.meta.url));
const parserPath = fileURLToPath(new URL("../syntaxes/tree-sitter-simi.wasm", import.meta.url));
const treeSitter = process.platform === "win32" ? "tree-sitter.cmd" : "tree-sitter";

execFileSync(treeSitter, ["build", "--wasm", "--output", parserPath, grammarPath], {
  stdio: "inherit",
});
chmodSync(parserPath, 0o644);
