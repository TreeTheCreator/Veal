# rows.ps1 -Project <folder> [-Rows 3,4,5,...]
# Runs plan-19's scriptable acceptance rows against the installed test build
# ("Veal Test" in %LOCALAPPDATA%\Veal Test) and prints one result line each.
# Close the real Veal first: the daemon pipe is per user. <folder> is pinned as
# the project if nothing is pinned yet; marker files go in its parent folder.
# Rows: 3 4 5 6 7 8 9 10 12 13 15 (11, 14 and 16 need a person).
param(
  [Parameter(Mandatory)][string]$Project,
  [int[]]$Rows = @(3, 4, 5, 6, 7, 8, 9, 10, 12, 13, 15),
  [int]$Port = 9333
)
$ErrorActionPreference = "Continue"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$cdp = Join-Path $here "cdp.mjs"
$exe = "$env:LOCALAPPDATA\Veal Test\veal.exe"
$Project = [IO.Path]::GetFullPath($Project)
$work = Split-Path -Parent $Project
$agentDir = Join-Path $work "fake-agent"
New-Item -ItemType Directory -Force $Project | Out-Null
& (Join-Path $here "fake-agent.ps1") $agentDir | Out-Null

# eval prints JSON: a string comes back unquoted, anything else as printed.
function Ev([string]$js) {
  $o = node $cdp $Port eval $js 2>$null
  if ("$o" -match '^".*"$') { "$o" | ConvertFrom-Json } else { "$o" }
}
function Typ([int]$pane, [string]$text) { node $cdp $Port type $pane $text; node $cdp $Port enter $pane }
# Exact label first; tab buttons carry their shortcut ("Terminal ^1").
function Click([string]$label) { Ev "(()=>{const bs=[...document.querySelectorAll('button')];(bs.find(b=>b.textContent.trim()=='$label')||bs.find(b=>b.textContent.trim().startsWith('$label'))).click()})()" | Out-Null }
function PaneCount { [int](Ev "document.querySelectorAll('.pane-term textarea').length") }
function Badges { Ev "[...document.querySelectorAll('.pane .badge')].map(b=>b.textContent.trim()).join(',')" }
function DaemonId { (Get-Process veal-daemon -EA SilentlyContinue).Id -join ',' }
function AgentId { (Get-Process claude -EA SilentlyContinue | Where-Object Path -like "$agentDir*").Id -join ',' }
function Gone { -not (Get-Process veal, veal-daemon -EA SilentlyContinue) }
function WaitFor([scriptblock]$cond, [double]$secs = 10) {
  $end = (Get-Date).AddSeconds($secs)
  while ((Get-Date) -lt $end) { if (& $cond) { return $true }; Start-Sleep -Milliseconds 200 }
  return [bool](& $cond)
}
function Launch {
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
  Start-Process $exe -WorkingDirectory $env:USERPROFILE
  WaitFor { try { (Ev "document.readyState") -match "complete" } catch { $false } } 20 | Out-Null
  Start-Sleep 3
}
function CloseWin { & (Join-Path $here "closewin.ps1") $exe | Out-Null }
function Marker([string]$name) { Join-Path $work "m-$name.txt" }
function NewPane { Click "+ New terminal"; Start-Sleep 6 }
function ClosePane([int]$i) { Ev "document.querySelectorAll('.pane')[$i].querySelector('button').click()" | Out-Null; Start-Sleep 1 }
function CloseMode([string]$label) { Click "Settings"; Start-Sleep 1; Click $label; Click "Terminal"; Start-Sleep 1 }
function StartAgent([int]$pane) {
  Typ $pane "& '$agentDir\claude.exe' -t 127.0.0.1"
  if (-not (WaitFor { AgentId } 8)) { Write-Warning "the stand-in agent did not start in pane $pane" }
  Start-Sleep 2
}
function Result([string]$row, [bool]$ok, [string]$detail) { "{0,-4} {1}  {2}" -f $row, ($(if ($ok) { "PASS" } else { "FAIL" })), $detail }
# Leaves the app running with exactly n panes, no agent, idle shells.
function Fresh([int]$n) {
  if (-not (Get-Process veal -EA SilentlyContinue)) { Launch }
  if ((Ev "document.body.innerText.includes('Pinned projects\n0')") -eq "true") {
    Click "Add project"; Start-Sleep 1
    $js = @"
(async()=>{const set=(el,v)=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,v);el.dispatchEvent(new Event('input',{bubbles:true}))};
const ins=[...document.querySelectorAll('form input')];const name=ins.find(i=>i.placeholder=='my-project');const root=ins.find(i=>i.placeholder.endsWith('my-project')&&i!==name);
set(name,'acceptance');set(root,String.raw``$Project``);await new Promise(r=>setTimeout(r,500));name.form.requestSubmit();await new Promise(r=>setTimeout(r,1000));return 1})()
"@
    Ev $js | Out-Null
  }
  Get-Process claude -EA SilentlyContinue | Where-Object Path -like "$agentDir*" | Stop-Process -Force
  Start-Sleep 1
  # Exited panes first (row 7 leaves one): typing into them does nothing.
  while ($true) {
    $dead = [int](Ev "[...document.querySelectorAll('.pane')].findIndex(p=>p.innerText.includes('process exited'))")
    if ($dead -lt 0) { break }
    ClosePane $dead
  }
  while ((PaneCount) -gt $n) { ClosePane 0 }
  while ((PaneCount) -lt $n) { NewPane }
  Start-Sleep 1
}

"Veal Test $((Get-Item $exe).VersionInfo.ProductVersion), rows $($Rows -join ' ')"

if ($Rows -contains 3) {
  Fresh 1; StartAgent 0
  $a = AgentId; $d = DaemonId
  Ev "location.reload()" | Out-Null; Start-Sleep 6; Ev "location.reload()" | Out-Null; Start-Sleep 6
  $b = Badges
  Result 3 (($a -ne "") -and (AgentId) -eq $a -and (DaemonId) -eq $d -and $b -eq "claude") "F5 twice: agent $a -> $(AgentId), daemon $d -> $(DaemonId), badge $b"
}

if ($Rows -contains 4) {
  if (-not (AgentId)) { Fresh 1; StartAgent 0 }
  $a = AgentId; $d = DaemonId
  Stop-Process -Name veal -Force; Start-Sleep 4
  $alive = (AgentId) -eq $a -and (DaemonId) -eq $d
  Launch; $b = Badges
  Result 4 ($alive -and (DaemonId) -eq $d -and $b -eq "claude") "app killed: agent and daemon alive=$alive; relaunch daemon $(DaemonId), badge $b"
}

if ($Rows -contains 5) {
  if (-not (AgentId)) { Fresh 1; StartAgent 0 }
  $d = DaemonId
  Stop-Process -Name veal-daemon -Force; Start-Sleep 3
  $msg = (Ev "document.body.innerText.includes('background service stopped')") -eq "true"
  Click "Restart"; Start-Sleep 7
  $m = Marker "row5"; Remove-Item $m -EA SilentlyContinue
  Typ 0 "(Get-Location).Path > '$m'"
  WaitFor { Test-Path $m } 5 | Out-Null
  $cwd = if (Test-Path $m) { (Get-Content $m).Trim() } else { "(none)" }
  $b = Badges
  Result 5 ($msg -and (DaemonId) -ne $d -and $cwd -eq $Project -and $b -eq "shell") "message=$msg, new daemon $(DaemonId) (was $d), cwd $cwd, badge $b"
}

if ($Rows -contains 6) {
  Fresh 2
  $t0 = Get-Date; CloseWin
  $gone = WaitFor { Gone } 30
  Result 6 $gone ("idle close: app and daemon gone in {0:n1} s" -f ((Get-Date) - $t0).TotalSeconds)
}

if ($Rows -contains 7) {
  Fresh 2
  Typ 1 "exit"; Start-Sleep 3
  $a = (PaneCount) -eq 1
  NewPane
  $m = Marker "row7b"; Remove-Item $m -EA SilentlyContinue
  Typ 1 "'ran' > '$m'; Start-Sleep 5; exit"
  WaitFor { Test-Path $m } 4 | Out-Null
  Stop-Process -Name veal -Force; Start-Sleep 7; Launch
  $b = (PaneCount) -eq 1
  NewPane
  $m = Marker "row7c"; Remove-Item $m -EA SilentlyContinue
  Typ 1 "'ran' > '$m'; Start-Sleep 5; exit 3"
  WaitFor { Test-Path $m } 4 | Out-Null
  Stop-Process -Name veal -Force; Start-Sleep 7; Launch
  $c = (Ev "document.body.innerText.includes('process exited (code 3)')") -eq "true"
  Result 7 ($a -and $b -and $c) "exit attached closes=$a; exit detached dropped=$b; exit 3 detached shows code=$c"
}

if ($Rows -contains 8 -or $Rows -contains 9) {
  Fresh 1; StartAgent 0; CloseMode "Keep running"
  $d = DaemonId; $a = AgentId
  $n = if ($Rows -contains 9) { 20 } else { 1 }
  $fails = @()
  for ($k = 1; $k -le $n; $k++) {
    Click "+ New terminal"; Start-Sleep -Milliseconds 300
    CloseWin
    $closed = WaitFor { -not (Get-Process veal -EA SilentlyContinue) } 10
    Launch; Start-Sleep 3
    $m = Marker "row9-$k"; Remove-Item $m -EA SilentlyContinue
    Typ 1 "'$k' > '$m'"
    $answered = WaitFor { Test-Path $m } 5
    $ok = $closed -and $answered -and (DaemonId) -eq $d
    if (-not $ok) { $fails += "$k(closed=$closed answered=$answered panes=$(PaneCount))" }
    ClosePane 1
  }
  $agentOk = $a -ne "" -and (AgentId) -eq $a
  $first = @($fails | Where-Object { $_ -like "1(*" }).Count -eq 0
  if ($Rows -contains 8) { Result 8 ($first -and $agentOk) "new terminal then close at once, Keep running: the pane answered on reopen, same daemon $d, agent $a kept=$agentOk" }
  if ($Rows -contains 9) { Result 9 ($fails.Count -eq 0 -and $agentOk) "$($n - $fails.Count)/$n closes went through and answered; failed: $($fails -join ',') ; agent kept=$agentOk" }
  CloseMode "Ask"
}

if ($Rows -contains 12) {
  if (-not (AgentId)) { Fresh 1; StartAgent 0 }
  CloseMode "Ask"
  CloseWin; Start-Sleep 2
  $dialog = Ev "(document.querySelector('[role=dialog], .modal')||{innerText:''}).innerText.includes('Agents running')"
  $focus = Ev "document.activeElement.textContent.trim()"
  Ev "[...document.querySelectorAll('button')].find(b=>b.textContent.trim()=='Keep running' && b.closest('[role=dialog], .modal')).click()" | Out-Null
  Start-Sleep 3
  $kept = -not (Get-Process veal -EA SilentlyContinue) -and (AgentId) -and (DaemonId)
  $log = Get-Content "$env:LOCALAPPDATA\com.veal.app\logs\daemon.log" -Tail 3
  $tray = [bool]($log -match "shown: Veal: 1 agent running")
  Launch; Start-Sleep 2
  $hidden = [bool]((Get-Content "$env:LOCALAPPDATA\com.veal.app\logs\daemon.log" -Tail 3) -match "hidden")
  CloseMode "Stop them"
  CloseWin
  $stopped = WaitFor { Gone } 10
  $agentGone = -not (AgentId)
  Launch; CloseMode "Ask"
  Result 12 ($dialog -eq "true" -and $focus -eq "Keep running" -and $kept -and $tray -and $hidden -and $stopped -and $agentGone) "ask dialog=$dialog focus=$focus; keep: agent kept=$([bool]$kept) tray shown=$tray hidden on reopen=$hidden; stop saved: no dialog, all gone=$stopped agent gone=$agentGone"
}

if ($Rows -contains 10) {
  Fresh 4
  0..3 | ForEach-Object { Remove-Item (Marker "flood$_"), (Marker "ans$_") -EA SilentlyContinue }
  for ($p = 0; $p -lt 4; $p++) {
    $m = Marker "flood$p"
    Typ $p "`$i=0; while(`$true){ `$i++; 'line ' + `$i + ' ................................................'; if(`$i % 2000 -eq 0){ `$i > '$m' } }"
  }
  $counts = @()
  foreach ($s in 1..3) { Start-Sleep 20; $counts += , @(0..3 | ForEach-Object { [int](Get-Content (Marker "flood$_") -EA SilentlyContinue) }) }
  $moving = $true
  for ($p = 0; $p -lt 4; $p++) { if (-not ($counts[0][$p] -gt 0 -and $counts[1][$p] -gt $counts[0][$p] -and $counts[2][$p] -gt $counts[1][$p])) { $moving = $false } }
  for ($p = 0; $p -lt 4; $p++) { node $cdp $Port ctrlc $p }
  Start-Sleep 1
  for ($p = 0; $p -lt 4; $p++) { Typ $p "'ok' > '$(Marker "ans$p")'" }
  $answered = WaitFor { @(0..3 | Where-Object { Test-Path (Marker "ans$_") }).Count -eq 4 } 15
  $m = Marker "pause10"; Remove-Item $m -EA SilentlyContinue
  Typ 0 "cls; 1..60000 | % { 'line ' + `$_ + ' ........................................' }; 'done' > '$m'; 'END-MARK'"
  Start-Sleep 1
  $pause = Start-Process node -ArgumentList $cdp, $Port, "pause", "30" -PassThru -NoNewWindow
  $whilePaused = WaitFor { Test-Path $m } 27
  $pause.WaitForExit(); Start-Sleep 5
  Result 10 ($moving -and $answered -and $whilePaused) "4 panes 60 s, lines at 20/40/60 s: $(($counts | ForEach-Object { $_ -join '/' }) -join ' | '); all answered after Ctrl+C=$answered; child finished while paused=$whilePaused (check the screen: last line 60000, END-MARK, prompt)"
  node $cdp $Port shot (Marker "row10-screen").Replace(".txt", ".png")
}

if ($Rows -contains 13) {
  Fresh 1
  $m = Marker "row13"; $c = Marker "row13-curl"; Remove-Item $m, $c -EA SilentlyContinue
  NewPane
  Typ 1 "`$env:https_proxy > '$m'; curl.exe -sI https://api.anthropic.com -o NUL -w '%{http_code}' > '$c'"
  WaitFor { (Test-Path $c) -and (Get-Content $c -Raw) } 15 | Out-Null
  $env_ = if (Test-Path $m) { (Get-Content $m).Trim() } else { "" }
  $code = if (Test-Path $c) { (Get-Content $c).Trim() } else { "" }
  Click "Settings"; Start-Sleep 1
  $row = Ev "(document.body.innerText.match(/(Follow system|Manual) \S (http[^\n]+)/)||[''])[0]"
  Click "Terminal"
  $same = ($env_ -eq "") -or ($row -match [regex]::Escape($env_))
  Result 13 ($same -and ($env_ -eq "" -or $code -match '^\d{3}$')) "pane https_proxy='$env_', curl via it: HTTP $code; Settings: $row"
}

if ($Rows -contains 15) {
  Fresh 4
  for ($p = 0; $p -lt 4; $p++) { Typ $p "cls; 1..5000 | % { 'line ' + `$_ + ' ..........................................' }" }
  Start-Sleep 15
  $d = Get-Process veal-daemon; $c0 = $d.TotalProcessorTime; $t0 = Get-Date
  Start-Sleep 60
  $d.Refresh()
  $core = ($d.TotalProcessorTime - $c0).TotalSeconds / ((Get-Date) - $t0).TotalSeconds * 100
  Result 15 ($core -lt 1) ("idle 60 s: daemon {0:n2}% of a core; private: daemon {1:n1} MB, veal.exe {2:n1} MB" -f $core, ($d.PrivateMemorySize64 / 1MB), ((Get-Process veal | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB))
  # Row 6 again, with full scrollback: closing still ends everything.
  CloseWin; Start-Sleep 2
  $dialog = Ev "(document.querySelector('[role=dialog], .modal')||{innerText:''}).innerText.replace(/\s+/g,' ')"
  $gone = WaitFor { Gone } 30
  Result "6b" $gone "close after row 15: all gone=$gone; dialog: '$dialog'"
}
