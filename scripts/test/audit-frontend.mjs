import { execFileSync } from "node:child_process";

const roots = JSON.parse(runPnpmList());
const packages = new Map();
for (const root of roots) {
  collectDependencies(root.dependencies);
  collectDependencies(root.devDependencies);
  collectDependencies(root.optionalDependencies);
}

// A resolved tree this large is expected; anything smaller means collection
// silently broke and the audit would "pass" without checking anything.
const MIN_EXPECTED_PACKAGES = 200;
if (packages.size < MIN_EXPECTED_PACKAGES) {
  throw new Error(
    `Frontend audit collected only ${packages.size} packages (expected >= ${MIN_EXPECTED_PACKAGES}); refusing to report a clean result`,
  );
}

const requestBody = Object.fromEntries(
  [...packages.entries()].map(([name, versions]) => [
    name,
    [...versions].sort(),
  ]),
);
const response = await fetch(
  "https://registry.npmjs.org/-/npm/v1/security/advisories/bulk",
  {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(requestBody),
  },
);
if (!response.ok) {
  throw new Error(
    `Frontend audit failed: HTTP ${response.status} ${await response.text()}`,
  );
}

const report = await response.json();
const advisories = Object.entries(report).flatMap(([name, items]) =>
  items.map((item) => ({ name, ...item })),
);
if (advisories.length === 0) {
  console.log(`No known vulnerabilities found in ${packages.size} packages`);
  process.exit(0);
}

const order = { critical: 0, high: 1, moderate: 2, low: 3, info: 4 };
advisories.sort(
  (left, right) =>
    (order[left.severity] ?? 5) - (order[right.severity] ?? 5) ||
    left.name.localeCompare(right.name),
);
for (const advisory of advisories) {
  console.error(
    `${advisory.severity.toUpperCase()} ${advisory.name}: ${advisory.title} (${advisory.url})`,
  );
}
process.exit(1);

function collectDependencies(dependencies) {
  if (!dependencies) return;
  for (const [name, dependency] of Object.entries(dependencies)) {
    if (dependency.version) {
      const versions = packages.get(name) ?? new Set();
      versions.add(dependency.version);
      packages.set(name, versions);
    }
    collectDependencies(dependency.dependencies);
    collectDependencies(dependency.optionalDependencies);
  }
}

function runPnpmList() {
  const pnpmScript = process.env.npm_execpath;
  if (pnpmScript) {
    return execFileSync(
      process.execPath,
      [pnpmScript, "list", "--json", "--depth", "Infinity"],
      {
        encoding: "utf8",
        maxBuffer: 32 * 1024 * 1024,
      },
    );
  }
  return execFileSync(
    process.platform === "win32" ? "pnpm.cmd" : "pnpm",
    ["list", "--json", "--depth", "Infinity"],
    {
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
      // Node >= 18.20.2 refuses to spawn .cmd shims without a shell.
      shell: true,
    },
  );
}
