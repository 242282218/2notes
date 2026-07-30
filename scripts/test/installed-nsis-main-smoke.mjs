#!/usr/bin/env node

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

/* global Buffer, WebSocket, setTimeout, clearTimeout */

const DEFAULT_TIMEOUT_MS = 15_000;
const POLL_INTERVAL_MS = 100;
const SELF_TEST_ARGS = [
  "--port",
  "9222",
  "--evidence-dir",
  ".tmp/installed-nsis-main-smoke-self-test",
  "--scenario-path",
  ".tmp/installed-nsis-main-smoke-self-test/scenario.json",
];

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.selfTest) {
    await selfTest();
    return;
  }

  await runScenario(options);
}

function parseArgs(argv) {
  const values = { selfTest: false, mode: "core" };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--self-test") {
      values.selfTest = true;
      continue;
    }
    if (!["--port", "--evidence-dir", "--scenario-path", "--mode"].includes(argument)) {
      throw new Error(`Unknown argument: ${argument}`);
    }
    const value = argv[index + 1];
    if (!value || value.startsWith("--")) {
      throw new Error(`Missing value for ${argument}`);
    }
    values[argument.slice(2).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase())] = value;
    index += 1;
  }

  if (values.selfTest && !values.port) return { ...parseArgs(SELF_TEST_ARGS), selfTest: true };
  const modes = new Set(["core", "import-select", "import-commit", "export-select", "backup-create", "backup-mutation", "backup-restore", "restart-verify"]);
  if (!modes.has(values.mode)) {
    throw new Error(`--mode must be one of: ${[...modes].join(", ")}`);
  }
  const port = Number(values.port);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error("--port must be an integer from 1 through 65535");
  }
  if (!values.evidenceDir || !values.scenarioPath) {
    throw new Error("Usage: node installed-nsis-main-smoke.mjs --port <port> --evidence-dir <dir> --scenario-path <file>");
  }
  return { ...values, port, evidenceDir: resolve(values.evidenceDir), scenarioPath: resolve(values.scenarioPath) };
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

async function waitFor(check, label, timeoutMs = DEFAULT_TIMEOUT_MS) {
  const deadline = Date.now() + timeoutMs;
  let lastValue;
  while (Date.now() < deadline) {
    lastValue = await check();
    if (lastValue) return lastValue;
    await delay(POLL_INTERVAL_MS);
  }
  throw new Error(`${label} timed out after ${timeoutMs}ms; last value: ${JSON.stringify(lastValue)}`);
}

async function fetchTargets(port) {
  const response = await fetch(`http://127.0.0.1:${port}/json`);
  if (!response.ok) throw new Error(`CDP target discovery failed: HTTP ${response.status}`);
  return response.json();
}

async function connectToMainTarget(port) {
  const target = await waitFor(
    async () => (await fetchTargets(port)).find((item) => item.type === "page" && item.webSocketDebuggerUrl),
    "main WebView2 CDP target",
  );
  return createCdpClient(target.webSocketDebuggerUrl);
}

function createCdpClient(url, { WebSocketCtor = WebSocket, requestTimeoutMs = DEFAULT_TIMEOUT_MS } = {}) {
  const socket = new WebSocketCtor(url);
  let requestId = 0;
  let terminalError = null;
  const pending = new Map();
  const events = [];

  function rejectPending(error) {
    for (const operation of pending.values()) {
      clearTimeout(operation.timer);
      operation.reject(error);
    }
    pending.clear();
  }

  function fail(error) {
    if (terminalError) return;
    terminalError = error instanceof Error ? error : new Error(String(error));
    rejectPending(terminalError);
  }

  const opened = new Promise((resolveOpen, rejectOpen) => {
    socket.addEventListener("open", resolveOpen, { once: true });
    socket.addEventListener("error", (event) => {
      const error = new Error(`CDP socket error: ${event?.message ?? "unknown error"}`);
      fail(error);
      rejectOpen(error);
    }, { once: true });
    socket.addEventListener("close", () => {
      const error = new Error("CDP socket closed");
      fail(error);
      rejectOpen(error);
    }, { once: true });
  });

  socket.addEventListener("message", ({ data }) => {
    const message = JSON.parse(data);
    if (message.id) {
      const operation = pending.get(message.id);
      if (!operation) return;
      pending.delete(message.id);
      clearTimeout(operation.timer);
      if (message.error) operation.reject(new Error(`${message.error.message}: ${JSON.stringify(message.error.data ?? null)}`));
      else operation.resolve(message.result);
      return;
    }
    events.push(message);
  });

  function call(method, params = {}) {
    if (terminalError) return Promise.reject(terminalError);
    const id = ++requestId;
    return new Promise((resolveCall, rejectCall) => {
      const timer = setTimeout(() => {
        if (!pending.delete(id)) return;
        rejectCall(new Error(`CDP call ${method} timed out after ${requestTimeoutMs}ms`));
      }, requestTimeoutMs);
      pending.set(id, { resolve: resolveCall, reject: rejectCall, timer });
      try {
        socket.send(JSON.stringify({ id, method, params }));
      } catch (error) {
        if (pending.delete(id)) clearTimeout(timer);
        rejectCall(error);
      }
    });
  }

  return {
    events,
    async ready() {
      await opened;
      await Promise.all([call("Runtime.enable"), call("Log.enable"), call("Page.enable"), call("DOM.enable")]);
    },
    call,
    close() {
      socket.close();
    },
  };
}

async function evaluate(client, expression) {
  const result = await client.call("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
    userGesture: false,
  });
  if (result.exceptionDetails) throw new Error(`DOM inspection failed: ${result.exceptionDetails.text}`);
  return result.result?.value;
}

async function query(client, selector) {
  return evaluate(client, `(() => {
    const node = document.querySelector(${JSON.stringify(selector)});
    if (!node) return null;
    const rect = node.getBoundingClientRect();
    const style = getComputedStyle(node);
    if (rect.width <= 0 || rect.height <= 0 || style.visibility === 'hidden' || style.display === 'none') return null;
    return { selector: ${JSON.stringify(selector)}, tagName: node.tagName, text: (node.textContent || '').trim(), ariaLabel: node.getAttribute('aria-label'), rect: { width: rect.width, height: rect.height } };
  })()`);
}

async function waitForSelector(client, selector, label = selector) {
  return waitFor(() => query(client, selector), label);
}

function isRetryableClickError(error) {
  return error instanceof Error && (
    error.message.startsWith("Could not compute box model.") ||
    error.message.startsWith("Could not find node with given id") ||
    error.message.startsWith("Click target not found:") ||
    error.message.startsWith("Click point does not hit target:")
  );
}

async function click(client, selector) {
  for (let attempt = 1; attempt <= 5; attempt += 1) {
    try {
      const documentRoot = await client.call("DOM.getDocument", { depth: 1 });
      const resolved = await client.call("DOM.querySelector", { nodeId: documentRoot.root.nodeId, selector });
      if (!resolved.nodeId) throw new Error(`Click target not found: ${selector}`);
      await client.call("DOM.scrollIntoViewIfNeeded", { nodeId: resolved.nodeId });
      const model = await client.call("DOM.getBoxModel", { nodeId: resolved.nodeId });
      const quad = model.model?.content ?? model.model?.border;
      if (!quad || quad.length < 8) throw new Error(`Click target has no box model: ${selector}`);
      const x = (quad[0] + quad[2] + quad[4] + quad[6]) / 4;
      const y = (quad[1] + quad[3] + quad[5] + quad[7]) / 4;
      const box = { left: Math.min(quad[0], quad[2], quad[4], quad[6]), right: Math.max(quad[0], quad[2], quad[4], quad[6]), top: Math.min(quad[1], quad[3], quad[5], quad[7]), bottom: Math.max(quad[1], quad[3], quad[5], quad[7]) };
      if (box.right <= box.left || box.bottom <= box.top) throw new Error(`Click target has zero-sized box: ${selector}`);
      const hit = await evaluate(client, `(() => {
        const target = document.querySelector(${JSON.stringify(selector)});
        const hit = document.elementFromPoint(${x}, ${y});
        return target === hit || target?.contains(hit) || hit?.contains(target);
      })()`);
      if (!hit) throw new Error(`Click point does not hit target: ${selector}`);
      await client.call("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
      await client.call("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
      return;
    } catch (error) {
      if (attempt === 5 || !isRetryableClickError(error)) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`CDP click failed for ${selector} after ${attempt} attempt(s): ${message}`, { cause: error });
      }
      await delay(POLL_INTERVAL_MS);
    }
  }
}

async function focusAndType(client, selector, text) {
  await waitForSelector(client, selector, `typing target ${selector}`);
  await click(client, selector);
  await client.call("Input.insertText", { text });
}

async function pressKey(client, key, modifiers = 0) {
  const specialKeys = {
    ArrowDown: { code: "ArrowDown", windowsVirtualKeyCode: 40 },
    ArrowUp: { code: "ArrowUp", windowsVirtualKeyCode: 38 },
    Backspace: { code: "Backspace", windowsVirtualKeyCode: 8 },
    Enter: { code: "Enter", windowsVirtualKeyCode: 13 },
    " ": { code: "Space", windowsVirtualKeyCode: 32 },
  };
  const definition = specialKeys[key] ?? {
    code: `Key${key.toUpperCase()}`,
    windowsVirtualKeyCode: key.toUpperCase().charCodeAt(0),
  };
  const { code, windowsVirtualKeyCode } = definition;
  const text = key === " " ? " " : key === "Enter" ? "\r" : undefined;
  await client.call("Input.dispatchKeyEvent", {
    type: "keyDown",
    key,
    code,
    windowsVirtualKeyCode,
    nativeVirtualKeyCode: windowsVirtualKeyCode,
    modifiers,
    text,
    unmodifiedText: text,
  });
  await client.call("Input.dispatchKeyEvent", {
    type: "keyUp",
    key,
    code,
    windowsVirtualKeyCode,
    nativeVirtualKeyCode: windowsVirtualKeyCode,
    modifiers,
  });
}

async function clearFocusedTextInput(client) {
  await pressKey(client, "a", 2);
  await pressKey(client, "Backspace");
}

async function findTreeNode(client, title) {
  return evaluate(client, `(() => {
    const node = Array.from(document.querySelectorAll('[role="treeitem"]')).find((item) => item.textContent?.trim() === ${JSON.stringify(title)});
    return node?.getAttribute('data-tree-node') ?? null;
  })()`);
}

async function selectTreeNode(client, title) {
  const nodeId = await waitFor(() => findTreeNode(client, title), `tree node ${title}`);
  const selector = `[role="treeitem"][data-tree-node=${JSON.stringify(nodeId)}]`;
  const selected = await evaluate(client, `document.querySelector(${JSON.stringify(selector)})?.getAttribute('aria-selected') === 'true'`);
  if (!selected) {
    await click(client, selector);
    await waitFor(() => evaluate(client, `document.querySelector(${JSON.stringify(selector)})?.getAttribute('aria-selected') === 'true'`), `selected tree node ${title}`);
  }
  return nodeId;
}

async function searchAndSelectExactTitle(client, title) {
  const searchInput = 'input[aria-label="搜索"]';
  await click(client, searchInput);
  await pressKey(client, "a", 2);
  await client.call("Input.insertText", { text: title });
  const entryId = await waitFor(() => evaluate(client, `(() => {
    const results = Array.from(document.querySelectorAll('button.entry-list-item[data-entry-id]')).filter(
      (item) => item.querySelector('strong')?.textContent?.trim() === ${JSON.stringify(title)},
    );
    return results.length === 1 ? results[0].getAttribute('data-entry-id') : null;
  })()`), `unique search result with exact title ${title}`);
  const selector = `button.entry-list-item[data-entry-id=${JSON.stringify(entryId)}]`;
  await click(client, selector);
  await waitFor(
    () => evaluate(client, `document.querySelector(${JSON.stringify(selector)})?.getAttribute('aria-current') === 'true'`),
    `current search entry ${entryId}`,
  );
  return entryId;
}

async function chooseDetailAction(client, label) {
  const direct = `[data-testid="detail-actions"] > button[aria-label=${JSON.stringify(label)}]`;
  if (await query(client, direct)) {
    await click(client, direct);
    return direct;
  }
  await click(client, '[data-testid="detail-actions"] button[aria-label="更多详情操作"]');
  const menuAction = `#detail-actions-menu button[aria-label=${JSON.stringify(label)}]`;
  await waitForSelector(client, menuAction, `responsive detail action ${label}`);
  await click(client, menuAction);
  return menuAction;
}

function snapshotExpression() {
  return `(() => ({
    href: location.href,
    title: document.title,
    viewport: { width: innerWidth, height: innerHeight },
    headings: Array.from(document.querySelectorAll('h1,h2')).map((node) => node.textContent?.trim()),
    activeElement: document.activeElement?.getAttribute('aria-label') || document.activeElement?.tagName,
    selectedTree: Array.from(document.querySelectorAll('[role="treeitem"][aria-selected="true"]')).map((node) => node.textContent?.trim()),
    listboxes: Array.from(document.querySelectorAll('[role="listbox"]')).map((node) => ({ id: node.id, text: node.textContent?.trim() })),
    dialogs: Array.from(document.querySelectorAll('[role="dialog"]')).map((node) => node.textContent?.trim()),
    bodyText: (document.body?.innerText || '').slice(0, 4000),
  }))()`;
}

async function capturePhase(client, evidenceDir, phase) {
  const snapshot = await evaluate(client, snapshotExpression());
  const screenshot = await client.call("Page.captureScreenshot", { format: "png" });
  await Promise.all([
    writeFile(resolve(evidenceDir, `${phase}.json`), `${JSON.stringify(snapshot, null, 2)}\n`),
    writeFile(resolve(evidenceDir, `${phase}.png`), Buffer.from(screenshot.data, "base64")),
  ]);
  return snapshot;
}

function isKnownWebViewCspError(event) {
  const entry = event.params?.entry;
  const frame = entry?.stackTrace?.callFrames?.[0];
  return event.method === "Log.entryAdded" && entry?.source === "security" && entry?.level === "error" && entry?.text?.startsWith("Applying inline style violates the following Content Security Policy directive 'style-src 'self''.") && entry.text.includes("The action has been blocked.") && frame?.functionName === "eT" && frame?.url.startsWith("http://tauri.localhost/assets/index-");
}

function consoleErrors(events) {
  return events
    .filter((event) => !isKnownWebViewCspError(event))
    .filter((event) => event.method === "Runtime.exceptionThrown" || (event.method === "Log.entryAdded" && event.params?.entry?.level === "error") || (event.method === "Runtime.consoleAPICalled" && event.params?.type === "error"))
    .map((event) => ({
      kind: event.method,
      text: event.params?.entry?.text ?? event.params?.exceptionDetails?.text ?? event.params?.args?.map((item) => item.value ?? item.description).join(" ") ?? "",
      source: event.params?.entry?.source ?? "runtime",
    }));
}

async function confirmDialog(client) {
  await waitForSelector(client, '[role="dialog"]', "confirmation dialog");
  await click(client, '[role="dialog"] button.btn-primary');
}

async function requireText(client, text, label) {
  const found = await evaluate(client, `document.body?.innerText?.includes(${JSON.stringify(text)}) === true`);
  if (!found) throw new Error(`${label}: expected visible text ${JSON.stringify(text)}`);
}

async function readScenario(scenarioPath) {
  try {
    const contents = await readFile(scenarioPath, "utf8");
    return JSON.parse(contents.replace(/^\uFEFF/, ""));
  } catch (error) {
    if (error?.code === "ENOENT") return {};
    throw error;
  }
}

async function waitForNativeSelection(client, label, postcondition) {
  await waitFor(postcondition, `${label} postcondition`, 30_000);
}

async function navigateToSettings(client) {
  await click(client, 'button[aria-label="设置"]');
  await waitFor(() => evaluate(client, `Array.from(document.querySelectorAll('h1,h2')).some((node) => node.textContent?.includes('设置'))`), "settings heading");
}

async function runSettingsMode(client, options, scenario) {
  const existing = await readScenario(options.scenarioPath);
  Object.assign(scenario, existing);
  if (!["backup-mutation", "restart-verify", "import-commit"].includes(options.mode)) {
    await navigateToSettings(client);
  }
  const selectors = {
    "import-select": 'button[aria-label="选择 Markdown 目录导入"]',
    "export-select": '#export button.btn-primary',
    "backup-create": '#backup button.btn-secondary',
  };
  if (options.mode === "backup-mutation") {
    const runId = scenario.runId;
    if (!runId) throw new Error("backup-mutation requires an existing core scenario runId");
    const title = `${runId} Backup Mutation`;
    await click(client, 'button[aria-label="收集箱"]');
    const searchInput = 'input[aria-label="搜索"]';
    await click(client, searchInput);
    await clearFocusedTextInput(client);
    await waitFor(() => evaluate(client, `document.querySelector(${JSON.stringify(searchInput)})?.value === ''`), "cleared backup mutation search");
    await waitForSelector(client, '.entry-list-item[data-entry-id]', "backup mutation entry list restored");
    await click(client, '[data-testid="create-entry"]');
    await waitFor(() => evaluate(client, `document.querySelector('input[aria-label="标题"]')?.value === ''`), "empty backup mutation title");
    await focusAndType(client, 'input[aria-label="标题"]', title);
    await waitFor(() => evaluate(client, `document.querySelector('input[aria-label="标题"]')?.value === ${JSON.stringify(title)}`), "backup mutation title");
    const afterBackupId = await waitFor(
      () => evaluate(client, `document.querySelector('.entry-list-item[aria-current="true"]')?.getAttribute('data-entry-id') ?? null`),
      "active backup mutation entry id",
    );
    await waitFor(
      () => evaluate(client, `Array.from(document.querySelectorAll('[role="status"]')).some((node) => node.textContent?.trim() === '已保存')`),
      "backup mutation autosave completion",
    );
    scenario.stages = {
      ...(scenario.stages ?? {}),
      backup: { ...(scenario.stages?.backup ?? {}), afterBackupId, afterBackupTitle: title },
    };
  } else if (options.mode === "restart-verify") {
    await click(client, 'button[aria-label="知识库"]');
    const expected = [scenario.inputs?.targetTitle, scenario.inputs?.sourceTitle, scenario.stages?.backup?.baselineTitle].filter(Boolean);
    for (const title of expected) {
      await waitFor(() => evaluate(client, `document.body?.innerText?.includes(${JSON.stringify(title)}) === true`), `restart preserved ${title}`);
    }
    const quickCaptureTitle = scenario.quickCapture?.title;
    if (typeof quickCaptureTitle !== "string" || !quickCaptureTitle) {
      throw new Error("restart-verify requires quickCapture.title from the independent quick-capture smoke");
    }
    scenario.quickCapture.entryId = await searchAndSelectExactTitle(client, quickCaptureTitle);
  } else if (options.mode === "import-commit") {
    const firstImport = !scenario.stages?.imports?.firstCommitted;
    const expectedResult = firstImport ? "导入完成：成功 5，跳过 0，失败 0" : "导入完成：成功 0，跳过 5，失败 0";
    await waitForSelector(client, '[role="dialog"]', "import confirmation dialog");
    await click(client, '[role="dialog"] button.btn-primary');
    await waitForNativeSelection(client, "import commit", () => evaluate(client, `document.body?.innerText?.includes(${JSON.stringify(expectedResult)}) === true`));
    scenario.stages = {
      ...(scenario.stages ?? {}),
      imports: {
        ...(scenario.stages?.imports ?? {}),
        expectedCount: 5,
        firstCommitted: true,
        firstResult: firstImport ? expectedResult : scenario.stages?.imports?.firstResult,
        repeatResult: firstImport ? scenario.stages?.imports?.repeatResult : expectedResult,
      },
    };
  } else if (options.mode === "backup-restore") {
    await waitForSelector(client, '[aria-label="可用备份"] button.btn-secondary', "available backup restore button");
    await click(client, '[aria-label="可用备份"] button.btn-secondary');
    await waitForSelector(client, '[role="dialog"]', "backup restore confirmation dialog");
    await click(client, '[role="dialog"] button.btn-primary');
    await waitForNativeSelection(client, "backup restore", () => evaluate(client, `document.body?.innerText?.includes('已恢复备份：') === true`));
  } else {
    const selector = selectors[options.mode];
    await click(client, selector);
    if (options.mode === "backup-create") {
      await waitForNativeSelection(client, "backup create", () => evaluate(client, `document.body?.innerText?.includes('已创建备份：') === true`));
      scenario.stages = { ...(scenario.stages ?? {}), backup: { ...(scenario.stages?.backup ?? {}), baselineTitle: scenario.inputs?.sourceTitle } };
    } else if (options.mode === "import-select") {
      await waitForSelector(client, '[role="dialog"]', "import preview confirmation dialog");
    } else {
      await waitForNativeSelection(client, "Markdown export", () => evaluate(client, `document.body?.innerText?.includes('已导出 ') === true`));
    }
  }
  scenario.phases.push({ name: options.mode, snapshot: await capturePhase(client, options.evidenceDir, options.mode) });
}

async function runScenario(options) {
  await mkdir(options.evidenceDir, { recursive: true });
  const client = await connectToMainTarget(options.port);
  const scenario = {
    startedAt: new Date().toISOString(),
    port: options.port,
    scenarioPath: options.scenarioPath,
    phases: [],
  };
  try {
    await client.ready();
    if (options.mode !== "core") {
      await runSettingsMode(client, options, scenario);
      scenario.finishedAt = new Date().toISOString();
      scenario.consoleErrors = consoleErrors(client.events);
      scenario.ok = scenario.consoleErrors.length === 0;
      await writeFile(options.scenarioPath, `${JSON.stringify(scenario, null, 2)}\n`);
      if (!scenario.ok) throw new Error(`Unexpected console error events: ${JSON.stringify(scenario.consoleErrors)}`);
      console.log(`PASS installed-nsis-main-smoke mode=${options.mode} evidence=${options.evidenceDir}`);
      return;
    }
    await waitForSelector(client, '[data-testid="create-entry"]', "stable create-entry selector");
    const runId = `NSIS_SMOKE_${Date.now()}`;
    let targetTitle = `${runId} Target`;
    const sourceTitle = `${runId} Source`;
    const heading = `${runId} Heading`;
    const undoToken = `${runId} Undo`;
    const missing = `${runId} Missing`;

    async function phase(name, action) {
      await action();
      scenario.phases.push({ name, snapshot: await capturePhase(client, options.evidenceDir, name) });
    }

    async function createKnowledgeEntry(title) {
      await click(client, '[data-testid="create-entry"]');
      const titleSelector = 'input[aria-label="标题"]';
      await waitForSelector(client, titleSelector);
      await waitFor(() => evaluate(client, `document.querySelector(${JSON.stringify(titleSelector)})?.value === ''`), `empty title for ${title}`);
      await focusAndType(client, titleSelector, title);
      await waitFor(() => evaluate(client, `document.querySelector(${JSON.stringify(titleSelector)})?.value === ${JSON.stringify(title)}`), `title value ${title}`);
      const entryId = await waitFor(() => evaluate(client, `document.querySelector('.entry-list-item[aria-current="true"]')?.getAttribute('data-entry-id') ?? null`), `active entry id ${title}`);
      await delay(700);
      await chooseDetailAction(client, "沉淀为知识");
      await delay(400);
      return entryId;
    }

    await phase("01-create-target", async () => {
      scenario.targetId = await createKnowledgeEntry(targetTitle);
    });

    await phase("02-create-source", async () => {
      scenario.sourceId = await createKnowledgeEntry(sourceTitle);
    });

    async function selectSourceInTree() {
      await click(client, 'button[aria-label="知识库"]');
      await waitForSelector(client, '[role="tree"]', "knowledge tree");
      await selectTreeNode(client, sourceTitle);
    }

    await phase("03-heading", async () => {
      await selectSourceInTree();
      const editor = '.ProseMirror[contenteditable="true"]';
      await waitForSelector(client, editor, "block editor");
      await waitFor(() => evaluate(client, `document.querySelector(${JSON.stringify(editor)})?.textContent?.trim() === ''`), "empty source editor");
      await focusAndType(client, editor, "#");
      await pressKey(client, " ");
      await client.call("Input.insertText", { text: heading });
      await pressKey(client, "Enter");
      await waitFor(() => evaluate(client, `Array.from(document.querySelectorAll('.ProseMirror h1,.ProseMirror h2')).some((node) => node.textContent?.trim() === ${JSON.stringify(heading)})`), "rendered source heading");
      await waitFor(() => evaluate(client, `Array.from(document.querySelectorAll('nav[aria-label="文档大纲"] button')).some((node) => node.textContent?.trim() === ${JSON.stringify(heading)})`), "outline heading");
    });

    await phase("04-wikilink", async () => {
      await selectSourceInTree();
      const editor = '.ProseMirror[contenteditable="true"]';
      const prefix = targetTitle.slice(0, Math.max(4, targetTitle.length - 7));
      await focusAndType(client, editor, `[[${prefix}`);
      const listbox = await waitForSelector(client, '[role="listbox"]', "WikiLink suggestion listbox");
      const targetOption = await waitFor(() => evaluate(client, `Array.from(document.querySelectorAll('[role="listbox"] [role="option"]')).find((node) => node.textContent?.trim().startsWith(${JSON.stringify(targetTitle)}))?.textContent?.trim() ?? null`), `WikiLink target option ${targetTitle}`);
      targetTitle = targetOption;
      const targetOptionIndex = await waitFor(() => evaluate(client, `Array.from(document.querySelectorAll('[role="listbox"] [role="option"]')).findIndex((node) => node.textContent?.trim() === ${JSON.stringify(targetTitle)})`), `WikiLink target option index ${targetTitle}`);
      if (targetOptionIndex < 0 || !listbox.text.includes(targetTitle)) throw new Error(`WikiLink target option absent: ${targetTitle}`);
      for (let index = 0; index < targetOptionIndex; index += 1) await pressKey(client, "ArrowDown");
      await pressKey(client, "Enter");
      try {
        await waitFor(() => evaluate(client, `document.querySelector('[role="listbox"]') === null && document.querySelector(${JSON.stringify(editor)})?.textContent?.includes(${JSON.stringify(`[[${targetTitle}]]`)})`), "canonical selected WikiLink");
      } catch (error) {
        const diagnostic = await evaluate(client, `(() => {
          const root = document.querySelector(${JSON.stringify(editor)});
          return {
            editorText: root?.textContent ?? null,
            editorHtml: root?.innerHTML ?? null,
            listbox: document.querySelector('[role="listbox"]')?.textContent?.trim() ?? null,
            activeElement: document.activeElement?.outerHTML ?? null,
            selection: getSelection()?.toString() ?? null,
          };
        })()`);
        await writeFile(resolve(options.evidenceDir, "04-wikilink-failure.json"), `${JSON.stringify(diagnostic, null, 2)}\n`);
        throw error;
      }
      await client.call("Input.insertText", { text: ` [[${missing}]] ${undoToken}` });
    });

    await phase("05-undo-redo", async () => {
      await pressKey(client, "z", 2);
      await pressKey(client, "z", 2 | 8);
      await requireText(client, undoToken, "redo restores editor content");
    });

    await phase("06-tree-outline", async () => {
      await click(client, 'button[aria-label="知识库"]');
      await selectTreeNode(client, sourceTitle);
      const sourceSelector = `[role="treeitem"][data-tree-node=${JSON.stringify(scenario.sourceId)}]`;
      const targetSelector = `[role="treeitem"][data-tree-node=${JSON.stringify(scenario.targetId)}]`;
      const targetDepth = Number(await evaluate(client, `document.querySelector(${JSON.stringify(targetSelector)})?.getAttribute('aria-level')`));
      await click(client, 'button[aria-label="移动到"]');
      const targetMenu = `[role="menu"] [role="menuitem"][data-tree-move-target=${JSON.stringify(scenario.targetId)}]`;
      await waitForSelector(client, targetMenu, `tree move target ${targetTitle}`);
      await click(client, targetMenu);
      const targetToggle = `[data-tree-toggle=${JSON.stringify(scenario.targetId)}]`;
      await waitForSelector(client, targetToggle, "target tree expand toggle");
      const targetExpanded = await evaluate(client, `document.querySelector(${JSON.stringify(targetToggle)})?.getAttribute('aria-expanded') === 'true'`);
      if (!targetExpanded) await click(client, targetToggle);
      await waitFor(() => evaluate(client, `document.querySelector(${JSON.stringify(sourceSelector)})?.getAttribute('aria-level') === ${JSON.stringify(String(targetDepth + 1))}`), "source tree depth after move");
      await requireText(client, targetTitle, "breadcrumb after tree move");
      const outline = 'nav[aria-label="文档大纲"] button';
      await waitForSelector(client, outline, "outline action");
      await click(client, outline);
      await waitFor(() => evaluate(client, `document.activeElement?.closest('.ProseMirror') !== null`), "outline focused editor block");
    });

    await phase("07-search-health", async () => {
      const searchInput = 'input[aria-label="搜索"]';
      const sourceSearchResult = `.entry-list-item[data-entry-id=${JSON.stringify(scenario.sourceId)}]`;
      await click(client, searchInput);
      await client.call("Input.insertText", { text: sourceTitle });
      await waitFor(() => evaluate(client, `(() => {
        const result = document.querySelector(${JSON.stringify(sourceSearchResult)});
        if (!result || !result.textContent?.includes(${JSON.stringify(sourceTitle)})) return null;
        return { sourceId: result.getAttribute('data-entry-id'), sourceTitle: result.textContent.trim() };
      })()`), `search result for sourceId ${scenario.sourceId} and sourceTitle ${sourceTitle}`);
      await click(client, sourceSearchResult);
      await waitFor(() => evaluate(client, `document.querySelector(${JSON.stringify(sourceSearchResult)})?.getAttribute('aria-current') === 'true'`), `current search entry ${scenario.sourceId}`);
      await click(client, 'button[aria-label="知识健康"]');
      await waitFor(() => evaluate(client, `Array.from(document.querySelectorAll('h1,h2')).some((node) => node.textContent?.includes('知识健康'))`), "knowledge health heading");
      const unresolvedKind = '[data-health-kind="unresolved_link"]';
      await waitForSelector(client, unresolvedKind, "unresolved WikiLink health category");
      await click(client, unresolvedKind);
      await waitFor(() => evaluate(client, `document.body?.innerText?.includes(${JSON.stringify(`[[${missing}]]`)}) === true`), "health shows unresolved WikiLink");
    });

    await phase("08-trash-restore", async () => {
      await click(client, 'button[aria-label="知识库"]');
      const targetToggle = `[data-tree-toggle=${JSON.stringify(scenario.targetId)}]`;
      await waitForSelector(client, targetToggle, "target tree toggle before trash");
      const targetExpanded = await evaluate(client, `document.querySelector(${JSON.stringify(targetToggle)})?.getAttribute('aria-expanded') === 'true'`);
      if (!targetExpanded) await click(client, targetToggle);
      await selectTreeNode(client, sourceTitle);
      await chooseDetailAction(client, "移到回收站");
      await confirmDialog(client);
      await click(client, 'button[aria-label="回收站"]');
      const sourceList = `.entry-list-item[data-entry-id=${JSON.stringify(scenario.sourceId)}]`;
      await waitForSelector(client, sourceList, "trashed source list item");
      await click(client, sourceList);
      await waitForSelector(client, '[data-testid="detail-actions"] > button[aria-label="恢复"]', "visible restore action");
      await chooseDetailAction(client, "恢复");
      await click(client, 'button[aria-label="知识库"]');
      await selectTreeNode(client, sourceTitle);
      await click(client, 'button[aria-label="移动到"]');
      await waitForSelector(client, '[role="menu"] [role="menuitem"]', "tree root move menu");
      const rootMenu = '[role="menu"] [role="menuitem"]:first-of-type';
      const rootItem = await waitFor(() => query(client, rootMenu), "tree root menu item");
      if (rootItem.text !== "根级") throw new Error(`tree root action mismatch: ${rootItem.text}`);
      await click(client, rootMenu);
      await waitFor(() => evaluate(client, `document.querySelector(${JSON.stringify(`[role="treeitem"][data-tree-node=${JSON.stringify(scenario.sourceId)}]`)})?.getAttribute('aria-level') === '1'`), "source returned to root");
    });

    scenario.runId = runId;
    scenario.inputs = { targetTitle, sourceTitle, heading, undo: undoToken, missing };
    scenario.finishedAt = new Date().toISOString();
    scenario.consoleErrors = consoleErrors(client.events);
    scenario.ok = scenario.consoleErrors.length === 0;
    await writeFile(options.scenarioPath, `${JSON.stringify(scenario, null, 2)}\n`);
    if (!scenario.ok) throw new Error(`Unexpected console error events: ${JSON.stringify(scenario.consoleErrors)}`);
    console.log(`PASS installed-nsis-main-smoke phases=${scenario.phases.length} evidence=${options.evidenceDir}`);
  } catch (error) {
    scenario.finishedAt = new Date().toISOString();
    scenario.ok = false;
    scenario.error = error instanceof Error ? error.stack : String(error);
    scenario.consoleErrors = consoleErrors(client.events);
    await writeFile(options.scenarioPath, `${JSON.stringify(scenario, null, 2)}\n`);
    throw error;
  } finally {
    client.close();
  }
}

// Helper-only self-test: verifies argument parsing, console-error classification,
// DOM snapshot expression, click-error classification, keyboard-event helpers,
// and CDP client timeout/close paths WITHOUT opening a real CDP connection or
// running the 8-phase main flow. It is NOT a substitute for end-to-end CDP
// runs in release.yml; add a real CDP smoke step there to cover the main flow.
async function selfTest() {
  const parsed = parseArgs(["--self-test"]);
  if (parsed.port !== 9222 || !parsed.evidenceDir.endsWith("installed-nsis-main-smoke-self-test")) {
    throw new Error("argument parsing self-test failed");
  }
  const failures = consoleErrors([
    { method: "Log.entryAdded", params: { entry: { level: "error", source: "javascript", text: "boom" } } },
    { method: "Log.entryAdded", params: { entry: { level: "warning", source: "javascript", text: "not an error" } } },
    { method: "Runtime.exceptionThrown", params: { exceptionDetails: { text: "uncaught" } } },
  ]);
  if (failures.length !== 2 || failures[0].text !== "boom" || failures[1].text !== "uncaught") {
    throw new Error("console error classification self-test failed");
  }
  if (!snapshotExpression().includes("selectedTree")) throw new Error("snapshot self-test failed");
  if (!isRetryableClickError(new Error("Click point does not hit target: button[aria-label=移动到]"))) {
    throw new Error("transient click hit-test classification self-test failed");
  }
  if (isRetryableClickError(new Error("Unexpected click failure"))) {
    throw new Error("non-retryable click error classification self-test failed");
  }

  const keyboardCalls = [];
  await clearFocusedTextInput({
    call(method, params) {
      keyboardCalls.push({ method, params });
    },
  });
  const [selectDown, selectUp, backspaceDown, backspaceUp] = keyboardCalls;
  if (
    keyboardCalls.length !== 4 ||
    selectDown.method !== "Input.dispatchKeyEvent" || selectDown.params.type !== "keyDown" || selectDown.params.key !== "a" || selectDown.params.modifiers !== 2 ||
    selectUp.params.type !== "keyUp" || selectUp.params.key !== "a" || selectUp.params.modifiers !== 2 ||
    backspaceDown.params.type !== "keyDown" || backspaceDown.params.key !== "Backspace" || backspaceDown.params.code !== "Backspace" || backspaceDown.params.windowsVirtualKeyCode !== 8 ||
    backspaceUp.params.type !== "keyUp" || backspaceUp.params.key !== "Backspace" || backspaceUp.params.code !== "Backspace" || backspaceUp.params.windowsVirtualKeyCode !== 8
  ) {
    throw new Error(`text input clearing self-test failed: ${JSON.stringify(keyboardCalls)}`);
  }

  const fakeSockets = [];
  class FakeSocket {
    constructor() {
      this.listeners = new Map();
      fakeSockets.push(this);
    }

    addEventListener(type, listener) {
      const listeners = this.listeners.get(type) ?? [];
      listeners.push(listener);
      this.listeners.set(type, listeners);
    }

    emit(type, event = {}) {
      for (const listener of this.listeners.get(type) ?? []) listener(event);
    }

    send() {}

    close() {
      this.emit("close");
    }
  }

  const timeoutClient = createCdpClient("ws://self-test", { WebSocketCtor: FakeSocket, requestTimeoutMs: 5 });
  const timeoutSocket = fakeSockets.at(-1);
  timeoutSocket.emit("open");
  await timeoutClient.call("Runtime.neverReplies").then(
    () => { throw new Error("CDP timeout self-test unexpectedly resolved"); },
    (error) => {
      if (!String(error.message).includes("timed out after 5ms")) throw error;
    },
  );
  timeoutSocket.close();

  const closeClient = createCdpClient("ws://self-test", { WebSocketCtor: FakeSocket, requestTimeoutMs: 1_000 });
  const closeSocket = fakeSockets.at(-1);
  closeSocket.emit("open");
  const pendingCall = closeClient.call("Runtime.pending");
  closeSocket.emit("close");
  await pendingCall.then(
    () => { throw new Error("CDP close self-test unexpectedly resolved"); },
    (error) => {
      if (error.message !== "CDP socket closed") throw error;
    },
  );
  console.log("PASS installed-nsis-main-smoke helper self-test (no real CDP)");
}

main().catch((error) => {
  console.error(`FAIL installed-nsis-main-smoke: ${error instanceof Error ? error.stack : String(error)}`);
  process.exitCode = 1;
});
