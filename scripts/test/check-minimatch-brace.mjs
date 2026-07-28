import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/**
 * Regression guard for the brace-expansion override:
 * global brace-expansion@5 breaks minimatch@9 (default export is not a function).
 * Walk the live dependency tree (not orphaned .pnpm store entries).
 */
function runPnpmListJson() {
  // Match audit-frontend.mjs: prefer npm_execpath (pnpm's JS entry) under node.
  const pnpmScript = process.env.npm_execpath;
  if (pnpmScript) {
    return execFileSync(
      process.execPath,
      [pnpmScript, "list", "--json", "--depth", "Infinity"],
      {
        cwd: root,
        encoding: "utf8",
        maxBuffer: 32 * 1024 * 1024,
      },
    );
  }
  // Git Bash on Windows often fails spawnSync("pnpm.cmd"); use shell there.
  return execFileSync("pnpm", ["list", "--json", "--depth", "Infinity"], {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    shell: true,
  });
}

function collectMinimatchPaths(dependencies, paths) {
  if (!dependencies) return;
  for (const [name, dependency] of Object.entries(dependencies)) {
    if (!dependency || typeof dependency !== "object") continue;
    const isMinimatch =
      name === "minimatch" || dependency.from === "minimatch" || dependency.name === "minimatch";
    if (isMinimatch && dependency.path) {
      paths.add(dependency.path);
    }
    collectMinimatchPaths(dependency.dependencies, paths);
    collectMinimatchPaths(dependency.optionalDependencies, paths);
  }
}

function checkInstall(installPath) {
  const packageJsonPath = path.join(installPath, "package.json");
  const require = createRequire(packageJsonPath);
  const pkg = require(packageJsonPath);
  const bracePkg = require("brace-expansion/package.json");
  const minimatch = require(installPath);
  const braceExpand =
    typeof minimatch.braceExpand === "function"
      ? minimatch.braceExpand
      : typeof minimatch.default?.braceExpand === "function"
        ? minimatch.default.braceExpand
        : null;
  if (!braceExpand) {
    throw new Error(
      `minimatch@${pkg.version} at ${installPath}: braceExpand export missing (brace-expansion@${bracePkg.version})`,
    );
  }
  let expanded;
  try {
    expanded = braceExpand("{a,b}.ts");
  } catch (error) {
    throw new Error(
      `minimatch@${pkg.version} + brace-expansion@${bracePkg.version} braceExpand failed: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
  const normalized = [...expanded].map(String).sort();
  const expected = ["a.ts", "b.ts"];
  if (normalized.length !== expected.length || normalized.some((v, i) => v !== expected[i])) {
    throw new Error(
      `minimatch@${pkg.version}: unexpected expansion ${JSON.stringify(expanded)} (brace-expansion@${bracePkg.version})`,
    );
  }
  const major = Number(String(pkg.version).split(".")[0]);
  const braceMajor = Number(String(bracePkg.version).split(".")[0]);
  if (major === 9 && braceMajor >= 5) {
    throw new Error(
      `minimatch@${pkg.version} must not resolve brace-expansion@${bracePkg.version} (API break)`,
    );
  }
  return {
    path: realpathSync(installPath),
    minimatch: pkg.version,
    braceExpansion: bracePkg.version,
  };
}

const roots = JSON.parse(runPnpmListJson());
const paths = new Set();
for (const rootNode of roots) {
  collectMinimatchPaths(rootNode.dependencies, paths);
  collectMinimatchPaths(rootNode.devDependencies, paths);
  collectMinimatchPaths(rootNode.optionalDependencies, paths);
}
if (paths.size === 0) {
  throw new Error("No minimatch packages found in pnpm dependency tree");
}

const results = [...paths]
  .sort()
  .map(checkInstall)
  .sort((a, b) => a.minimatch.localeCompare(b.minimatch) || a.path.localeCompare(b.path));

for (const result of results) {
  console.log(
    `ok minimatch@${result.minimatch} brace-expansion@${result.braceExpansion} (${result.path})`,
  );
}
console.log(`Checked ${results.length} live minimatch install(s)`);
