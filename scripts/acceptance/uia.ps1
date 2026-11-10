# uia.ps1 list <pid>            -> windows of that process and their named elements
# uia.ps1 click <pid> <name>    -> BM_CLICK on the first element with that name
param([string]$cmd, [int]$procId, [string]$name)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -Namespace W -Name U2 -MemberDefinition '[DllImport("user32.dll")] public static extern bool PostMessage(System.IntPtr h, uint m, System.IntPtr w, System.IntPtr l);'
$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::ProcessIdProperty), $procId
$wins = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)
foreach ($w in $wins) {
  $all = $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
  if ($cmd -eq "list") {
    "== window: $($w.Current.Name)"
    foreach ($e in $all) {
      $t = $e.Current.ControlType.ProgrammaticName -replace 'ControlType\.', ''
      if ($e.Current.Name) { "  [$t] $(($e.Current.Name -split "`n")[0])" }
    }
  }
  if ($cmd -eq "click") {
    foreach ($e in $all) {
      if ($e.Current.Name -eq $name -and $e.Current.NativeWindowHandle -ne 0) {
        [W.U2]::PostMessage([IntPtr]$e.Current.NativeWindowHandle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
        "clicked $name"; exit 0
      }
    }
  }
}
if ($cmd -eq "click") { "no element $name"; exit 1 }
