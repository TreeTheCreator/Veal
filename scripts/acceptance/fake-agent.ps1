# fake-agent.ps1 <dir>
# Makes <dir>\claude.exe, a stand-in agent: a copy of ping.exe, which runs
# until killed with `claude.exe -t 127.0.0.1`. Put <dir> first on PATH for
# the shell, then run it in a pane; Veal counts it as an agent.
param([Parameter(Mandatory)][string]$dir)
New-Item -ItemType Directory -Force $dir | Out-Null
Copy-Item "$env:SystemRoot\System32\PING.EXE" (Join-Path $dir "claude.exe") -Force
"made $(Join-Path $dir 'claude.exe'); run: claude.exe -t 127.0.0.1"
