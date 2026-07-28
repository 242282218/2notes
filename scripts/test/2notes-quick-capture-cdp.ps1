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

$stdoutPath = Join-Path (Get-Location) "tmp-2notes-cdp-stdout.log"
$stderrPath = Join-Path (Get-Location) "tmp-2notes-cdp-stderr.log"
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

async function targets() {
  const response = await fetch(`http://127.0.0.1:${port}/json`);
  return await response.json();
}

async function waitForTarget(predicate, timeoutMs = 10000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const found = (await targets()).find(predicate);
    if (found) {
      return found;
    }
    await sleep(250);
  }
  throw new Error("target timeout");
}

async function connect(url) {
  const ws = new WebSocket(url);
  await new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve, { once: true });
    ws.addEventListener("error", reject, { once: true });
  });

  let id = 0;
  const pending = new Map();

  ws.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const callbacks = pending.get(message.id);
      pending.delete(message.id);
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
    ws.send(JSON.stringify({ id: callId, method, params }));
    return new Promise((resolve, reject) => {
      pending.set(callId, { resolve, reject });
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
  return { ws, evaluate };
}

async function stableConnect(predicate) {
  for (let attempt = 0; attempt < 5; attempt += 1) {
    const target = await waitForTarget(predicate);
    await sleep(700);
    try {
      const client = await connect(target.webSocketDebuggerUrl);
      await client.evaluate("document.readyState");
      return client;
    } catch {
      await sleep(500);
    }
  }
  throw new Error("stable connect failed");
}

const main = await stableConnect(
  (target) => target.type === "page" && !target.url.includes("quick-capture"),
);

const mainReady = await main.evaluate(`
  new Promise((resolve) => {
    const findRecordButton = () => {
      const byText = Array.from(document.querySelectorAll('button.btn-primary')).find((button) =>
        (button.textContent || '').includes('记录'),
      );
      if (byText) {
        return byText;
      }
      // Inbox empty state has a single topbar primary action.
      return document.querySelector('header button.btn-primary, button.btn-primary');
    };
    if (findRecordButton()) {
      resolve(true);
      return;
    }
    const observer = new MutationObserver(() => {
      if (findRecordButton()) {
        observer.disconnect();
        resolve(true);
      }
    });
    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
      characterData: true,
    });
    setTimeout(() => resolve(Boolean(findRecordButton())), 15000);
  })
`);

console.log("MAIN_READY", mainReady);
if (!mainReady) {
  const snapshot = await main.evaluate(`({
    href: location.href,
    ready: document.readyState,
    bodyText: (document.body && document.body.innerText || '').slice(0, 500),
    buttons: Array.from(document.querySelectorAll('button')).map((button) => ({
      text: (button.textContent || '').trim(),
      className: button.className,
    })),
  })`);
  console.log("MAIN_SNAPSHOT", JSON.stringify(snapshot));
  throw new Error("main record button missing");
}

await main.evaluate(`
  (() => {
    const button =
      Array.from(document.querySelectorAll('button.btn-primary')).find((item) =>
        (item.textContent || '').includes('记录'),
      ) || document.querySelector('header button.btn-primary, button.btn-primary');
    if (!button) {
      throw new Error('record button disappeared');
    }
    button.click();
  })()
`);

const quick = await stableConnect((target) =>
  target.url.includes("view=quick-capture"),
);

const hasTextarea = await quick.evaluate(`
  new Promise((resolve) => {
    if (document.querySelector('#quick-capture-content, textarea')) {
      resolve(true);
      return;
    }
    const observer = new MutationObserver(() => {
      if (document.querySelector('#quick-capture-content, textarea')) {
        observer.disconnect();
        resolve(true);
      }
    });
    observer.observe(document.documentElement, { childList: true, subtree: true });
    setTimeout(
      () => resolve(Boolean(document.querySelector('#quick-capture-content, textarea'))),
      4000,
    );
  })
`);

if (!hasTextarea) {
  throw new Error("quick capture textarea missing");
}

const noteJson = JSON.stringify(note);
const fillResult = await quick.evaluate(
  "(() => {" +
    "const textarea = document.querySelector('#quick-capture-content, textarea');" +
    "if (!textarea) throw new Error('textarea missing');" +
    "textarea.focus();" +
    "const setter = Object.getOwnPropertyDescriptor(window.HTMLTextAreaElement.prototype, 'value').set;" +
    "setter.call(textarea, " + noteJson + ");" +
    "textarea.dispatchEvent(new Event('input', { bubbles: true }));" +
    "return textarea.value;" +
  "})()",
);
console.log("FILL_RESULT", fillResult);

await sleep(1000);

const saveState = await quick.evaluate(
  "(() => {" +
    "const buttons = Array.from(document.querySelectorAll('button')).map((item) => ({" +
      "text: (item.textContent || '').replace(/\\s+/g, ' ').trim()," +
      "className: item.className," +
      "disabled: item.disabled," +
      "type: item.type," +
    "}));" +
    "const button = Array.from(document.querySelectorAll('button')).find((item) => {" +
      "const text = (item.textContent || '').replace(/\\s+/g, ' ').trim();" +
      "return text.includes('保存') && item.className.includes('btn-primary');" +
    "});" +
    "const textarea = document.querySelector('#quick-capture-content, textarea');" +
    "return {" +
      "hasButton: Boolean(button)," +
      "disabled: button ? button.disabled : null," +
      "buttonText: button ? (button.textContent || '').replace(/\\s+/g, ' ').trim() : null," +
      "value: textarea ? textarea.value : null," +
      "buttons," +
    "};" +
  "})()",
);
console.log("SAVE_STATE", JSON.stringify(saveState));

// Prefer Enter submit because QuickCapture binds keydown.enter.exact to submit.
await quick.evaluate(
  "(() => {" +
    "const textarea = document.querySelector('#quick-capture-content, textarea');" +
    "if (!textarea) throw new Error('textarea missing before submit');" +
    "textarea.focus();" +
    "const event = new KeyboardEvent('keydown', {" +
      "key: 'Enter'," +
      "code: 'Enter'," +
      "keyCode: 13," +
      "which: 13," +
      "bubbles: true," +
      "cancelable: true," +
    "});" +
    "textarea.dispatchEvent(event);" +
    "const button = Array.from(document.querySelectorAll('button')).find((item) => {" +
      "const text = (item.textContent || '').replace(/\\s+/g, ' ').trim();" +
      "return text.includes('保存') && item.className.includes('btn-primary') && !item.disabled;" +
    "});" +
    "if (button) button.click();" +
    "return Boolean(button);" +
  "})()",
);

await sleep(2000);

const afterText = await main.evaluate("document.body.innerText");
const mainHasNote = afterText.includes(note);

console.log("NOTE", note);
console.log("MAIN_HAS_NOTE", mainHasNote);
console.log("EVENT_COUNT", events.length);
console.log("EVENTS", JSON.stringify(events.slice(0, 10)));

main.ws.close();
quick.ws.close();

if (!mainHasNote || events.length > 0) {
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
    Stop-Process -Id $process.Id -Force
  }

  [System.IO.File]::WriteAllText($stdoutPath, $process.StandardOutput.ReadToEnd())
  [System.IO.File]::WriteAllText($stderrPath, $process.StandardError.ReadToEnd())

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
