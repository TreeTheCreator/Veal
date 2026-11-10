# closewin.ps1 [exe-path]
# Sends WM_CLOSE to Veal's main window, as the title bar's X does. With a path,
# only the veal.exe running from that path (the installed test build, say).
param([string]$exe)
Add-Type -Namespace W -Name U -MemberDefinition '[DllImport("user32.dll")] public static extern bool PostMessage(System.IntPtr h, uint m, System.IntPtr w, System.IntPtr l);'
if ($exe) { $exe = [System.IO.Path]::GetFullPath($exe) }
$p = Get-Process veal -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 -and (-not $exe -or $_.Path -eq $exe) } | Select-Object -First 1
if (-not $p) { "no veal window"; exit 1 }
[W.U]::PostMessage($p.MainWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
"closed $($p.Id)"
