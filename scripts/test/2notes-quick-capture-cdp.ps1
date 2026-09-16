param(
  [string]$ExePath = "src-tauri\target\release\two_notes.exe",
  [string]$AppDataRoot = ".tmp\2notes-cdp-appdata",
  [int]$CdpPort = 9333,
  [switch]$KeepTestData
)

$ErrorActionPreference = "Stop"

function Resolve-RepoPath([string]$Path) {
  if ([System.IO.Path]::IsPathRooted($Path)) {
    return $Path
  }
  return Join-Path (Get-Location) $Path
}

function Get-TestDatabasePath([string]$Root) {
  return Join-Path $Root "data\2notes.sqlite"
}

function Invoke-SqliteCleanup([string]$DbPath, [string]$Pattern) {
  $python = @"
import os
import sqlite3

db_path = r"$DbPath"
if not os.path.exists(db_path):
    print("DB_CLEANUP skipped: database missing")
    raise SystemExit(0)

con = sqlite3.connect(db_path, timeout=2)
cur = con.cursor()
cur.execute("delete from entries where current_content like ?", (r"$Pattern%",))
cur.execute("delete from drafts where id = 'quick_capture' and content like ?", (r"$Pattern%",))
con.commit()
remaining = list(cur.execute(
    "select id, current_content from entries where current_content like ?",
    (r"$Pattern%",),
))
print("DB_CLEANUP remaining_test_entries=", remaining)
con.close()
"@
  $python | python -
}

$resolvedExe = Resolve-RepoPath $ExePath
$resolvedAppData = Resolve-RepoPath $AppDataRoot
$dbPath = Get-TestDatabasePath $resolvedAppData
if (-not (Test-Path -LiteralPath $resolvedExe)) {
  throw "Executable not found: $resolvedExe. Run pnpm run tauri:build first."
}
New-Item -ItemType Directory -Force -Path $resolvedAppData | Out-Null

$logDirectory = Join-Path (Get-Location) ".tmp"
New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
$stdoutPath = Join-Path $logDirectory "2notes-cdp-stdout.log"
$stderrPath = Join-Path $logDirectory "2notes-cdp-stderr.log"
Remove-Item -LiteralPath $stdoutPath, $stderrPath -ErrorAction SilentlyContinue

$psi = [System.Diagnostics.ProcessStartInfo]::new()
$psi.FileName = $resolvedExe
$psi.WorkingDirectory = (Get-Location).Path
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.Environment["TWONOTES_TEST_ROOT"] = $resolvedAppData
$psi.Environment["RUST_LOG"] = "trace"
$psi.Environment["RUST_BACKTRACE"] = "1"
$psi.Environment["WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"] =
  "--remote-debugging-port=$CdpPort --enable-logging --v=1"

$process = [System.Diagnostics.Process]::Start($psi)
$notePrefix = "CDP_TEST_$([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())"
$nodeExit = 1
$testPassed = $false

try {
  # Allow Vue shell, settings hydrate, and window-state restore to settle.
  Start-Sleep -Seconds 6

  $env:TWONOTES_CDP_PORT = [string]$CdpPort
  $env:TWONOTES_NOTE_PREFIX = $notePrefix

  $nodeScript = @'
const port = Number(process.env.TWONOTES_CDP_PORT);
const note = `${process.env.TWONOTES_NOTE_PREFIX} quick capture refresh`;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const events = [];
const cdpTimeoutMs = 15000;

let lastTargetSnapshot = null;
let lastTargetError = null;

async function targets() {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/json`);
    const payload = await response.json();
    lastTargetSnapshot = payload.map((target) => ({
      type: target.type,
      title: target.title,
      url: target.url,
      hasWebSocket: Boolean(target.webSocketDebuggerUrl),
    }));
    lastTargetError = null;
    return payload;
  } catch (error) {
    lastTargetError = error instanceof Error ? error.message : String(error);
    return [];
  }
}

async function waitForTarget(predicate, timeoutMs = 10000, label = "target") {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const found = (await targets()).find(predicate);
    if (found) {
      return found;
    }
    await sleep(250);
  }
  throw new Error(
    `${label} timeout: targets=${JSON.stringify(lastTargetSnapshot)} error=${lastTargetError}`,
  );
}

async function connect(url) {
  const ws = new WebSocket(url);
  await new Promise((resolve, reject) => {
    let settled = false;
    const settle = (callback, value) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      ws.removeEventListener("open", onOpen);
      ws.removeEventListener("close", onClose);
      ws.removeEventListener("error", onError);
      callback(value);
    };
    const onOpen = () => settle(resolve);
    const onClose = () => settle(reject, new Error("CDP WebSocket closed before opening"));
    const onError = () => settle(reject, new Error("CDP WebSocket failed before opening"));
    const timer = setTimeout(
      () => settle(reject, new Error(`CDP WebSocket open timed out after ${cdpTimeoutMs}ms`)),
      cdpTimeoutMs,
    );
    ws.addEventListener("open", onOpen);
    ws.addEventListener("close", onClose);
    ws.addEventListener("error", onError);
  });

  let id = 0;
  const pending = new Map();

  function rejectPending(error) {
    for (const { reject, timer } of pending.values()) {
      clearTimeout(timer);
      reject(error);
    }
    pending.clear();
  }

  ws.addEventListener("close", () => rejectPending(new Error("CDP WebSocket closed")));
  ws.addEventListener("error", () => rejectPending(new Error("CDP WebSocket error")));

  ws.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const callbacks = pending.get(message.id);
      pending.delete(message.id);
      clearTimeout(callbacks.timer);
      if (message.error) {
        callbacks.reject(new Error(JSON.stringify(message.error)));
      } else {
        callbacks.resolve(message.result);
      }
      return;
    }
    if (
      message.method === "Runtime.exceptionThrown" ||
      message.method === "Log.entryAdded" ||
      message.method === "Runtime.consoleAPICalled"
    ) {
      events.push(message);
    }
  });

  function call(method, params = {}) {
    const callId = ++id;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (pending.delete(callId)) {
          reject(new Error(`CDP call ${method} timed out after ${cdpTimeoutMs}ms`));
        }
      }, cdpTimeoutMs);
      pending.set(callId, { resolve, reject, timer });
      try {
        if (ws.readyState !== WebSocket.OPEN) {
          throw new Error("CDP WebSocket is not open");
        }
        ws.send(JSON.stringify({ id: callId, method, params }));
      } catch (error) {
        if (pending.delete(callId)) {
          clearTimeout(timer);
          reject(error);
        }
      }
    });
  }

  async function evaluate(expression) {
    const result = await call("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (result.exceptionDetails) {
      throw new Error(JSON.stringify(result.exceptionDetails));
    }
    return result.result?.value;
  }

  await call("Runtime.enable");
  await call("Log.enable");
  await call("DOM.enable");
  return { ws, call, evaluate };
}

async function stableConnect(predicate, label) {
  let lastConnectionError = null;
  for (let attempt = 0; attempt < 5; attempt += 1) {
    const target = await waitForTarget(predicate, 10000, label);
    if (!target.webSocketDebuggerUrl) {
      lastConnectionError = `${label} has no WebSocket debugger URL`;
      await sleep(500);
      continue;
    }
    await sleep(700);
    try {
      const client = await connect(target.webSocketDebuggerUrl);
      await client.evaluate("document.readyState");
      return client;
    } catch (error) {
      lastConnectionError = error instanceof Error ? error.message : String(error);
      await sleep(500);
    }
  }
  throw new Error(
    `${label} stable connect failed: targets=${JSON.stringify(lastTargetSnapshot)} error=${lastConnectionError}`,
  );
}

async function resolveSelector(client, selector) {
  const { root } = await client.call("DOM.getDocument", { depth: 0, pierce: true });
  const { nodeId } = await client.call("DOM.querySelector", {
    nodeId: root.nodeId,
    selector,
  });
  if (!nodeId) {
    return null;
  }

  const { model } = await client.call("DOM.getBoxModel", { nodeId });
  const quad = model.content.length === 8 ? model.content : model.border;
  return {
    nodeId,
    x: (quad[0] + quad[2] + quad[4] + quad[6]) / 4,
    y: (quad[1] + quad[3] + quad[5] + quad[7]) / 4,
  };
}

async function waitForSelector(client, selector, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const target = await resolveSelector(client, selector);
      if (target) {
        return target;
      }
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
    }
    await sleep(100);
  }
  throw new Error(`${label} missing: selector=${selector} error=${lastError}`);
}

async function clickSelector(client, selector, label) {
  const target = await waitForSelector(client, selector, 15000, label);
  await client.call("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: target.x,
    y: target.y,
    button: "left",
    clickCount: 1,
  });
  await client.call("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: target.x,
    y: target.y,
    button: "left",
    clickCount: 1,
  });
  return target;
}

async function clickFirstSelector(client, selectors, label) {
  for (const selector of selectors) {
    try {
      return await clickSelector(client, selector, label);
    } catch (error) {
      if (!String(error).includes(" missing:")) {
        throw error;
      }
    }
  }
  throw new Error(`${label} missing: selectors=${JSON.stringify(selectors)}`);
}

const main = await stableConnect(
  (target) => target.type === "page" && !target.url.includes("quick-capture"),
  "main target",
);

await waitForSelector(main, 'button[aria-label="\u5feb\u901f\u8bb0\u5f55"]', 15000, "main quick capture button");
await clickSelector(main, 'button[aria-label="\u5feb\u901f\u8bb0\u5f55"]', "main quick capture button");

const quick = await stableConnect(
  (target) => target.type === "page" && target.url !== "http://tauri.localhost/",
  "quick capture target",
);

await clickFirstSelector(
  quick,
  ["#quick-capture-content", "textarea"],
  "quick capture textarea",
);
await quick.call("Input.insertText", { text: note });

await sleep(1000);

const saveState = await quick.evaluate(`
  (() => {
    const primaryButtons = Array.from(document.querySelectorAll('button.btn-primary')).map((item) => ({
      text: (item.textContent || '').replace(/\s+/g, ' ').trim(),
      ariaLabel: item.getAttribute('aria-label'),
      disabled: item.disabled,
      type: item.type,
    }));
    const textarea = document.querySelector('#quick-capture-content, textarea');
    return {
      ariaSavePresent: Boolean(document.querySelector('button[aria-label="保存"]')),
      primaryButtons,
      value: textarea ? textarea.value : null,
    };
  })()
`);
console.log("SAVE_STATE", JSON.stringify(saveState));

let saveSelector = 'button[aria-label="保存"]';
if (!saveState.ariaSavePresent) {
  if (saveState.primaryButtons.length !== 1) {
    throw new Error(`save button is ambiguous: ${JSON.stringify(saveState.primaryButtons)}`);
  }
  saveSelector = "button.btn-primary";
}
await clickSelector(quick, saveSelector, "quick capture save button");

await sleep(2000);

const afterText = await main.evaluate("document.body.innerText");
const mainHasNote = afterText.includes(note);

console.log("QUICK_CAPTURE_TITLE=" + note);
console.log("NOTE", note);
console.log("MAIN_HAS_NOTE", mainHasNote);
console.log("EVENT_COUNT", events.length);
console.log("EVENTS", JSON.stringify(events.slice(0, 10)));

main.ws.close();
quick.ws.close();

const unexpectedEvents = events;
console.log("UNEXPECTED_EVENT_COUNT", unexpectedEvents.length);
console.log("UNEXPECTED_EVENTS", JSON.stringify(unexpectedEvents.slice(0, 10)));

if (!mainHasNote || unexpectedEvents.length > 0) {
  process.exit(1);
}
'@

  $nodeScript | node --input-type=module -
  $nodeExit = $LASTEXITCODE

  $alive = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
  if ($alive) {
    Write-Output "PROCESS responding=$($alive.Responding) title=$($alive.MainWindowTitle)"
  } else {
    Write-Output "PROCESS exited"
  }

  $dbCheck = @"
import sqlite3

db_path = r"$dbPath"
con = sqlite3.connect(db_path, timeout=2)
cur = con.cursor()
rows = list(cur.execute(
    "select id, current_content from entries where current_content like ?",
    (r"$notePrefix%",),
))
drafts = list(cur.execute(
    "select id, content, revision from drafts where id = 'quick_capture'",
))
print("DB_TEST_ROWS", rows)
print("DB_DRAFTS", drafts)
con.close()
raise SystemExit(0 if len(rows) == 1 else 1)
"@
  $dbCheck | python -
  $dbExit = $LASTEXITCODE

  if ($nodeExit -ne 0 -or $dbExit -ne 0) {
    throw "Quick capture CDP smoke failed: nodeExit=$nodeExit dbExit=$dbExit"
  }

  $testPassed = $true
  Write-Output "PASS quick_capture_cdp notePrefix=$notePrefix"
} finally {
  $alive = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
  if ($alive) {
    # Give the process a chance to flush stdout/stderr before forcing it down,
    # so ReadToEnd below captures the full buffer instead of a partial one.
    if (-not $process.WaitForExit(3000)) {
      Stop-Process -Id $process.Id -Force
      $process.WaitForExit(2000) | Out-Null
    }
  } else {
    # Process already exited on its own; ensure async readers are drained.
    $process.WaitForExit(2000) | Out-Null
  }

  # ReadToEnd blocks until the redirected stream closes. Using the async API
  # keeps the disk write outside any lock and matches installed-nsis-smoke.ps1.
  $stdoutTask = $process.StandardOutput.ReadToEndAsync()
  $stderrTask = $process.StandardError.ReadToEndAsync()
  [System.IO.File]::WriteAllText($stdoutPath, $stdoutTask.GetAwaiter().GetResult())
  [System.IO.File]::WriteAllText($stderrPath, $stderrTask.GetAwaiter().GetResult())

  Write-Output "STDOUT_LOG $stdoutPath"
  Write-Output "STDERR_LOG $stderrPath"

  if (-not $KeepTestData) {
    Invoke-SqliteCleanup $dbPath $notePrefix
  }

  if ($testPassed -and -not $KeepTestData) {
    Remove-Item -LiteralPath $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
    $workspacePrefix = [System.IO.Path]::GetFullPath((Get-Location).Path) + [System.IO.Path]::DirectorySeparatorChar
    if ($resolvedAppData.StartsWith($workspacePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
      Remove-Item -LiteralPath $resolvedAppData -Recurse -Force
    }
    Write-Output "CLEANUP complete"
  }
}
