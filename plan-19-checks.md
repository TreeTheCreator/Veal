# plan-19 checks for a person

These need a real `claude`, the real taskbar, a second account, sleep, or a
clean PC. Steps 5–8 checked everything else with scripts. Write the result in
each box: **pass**, or what happened instead. Send the file back when done.

**Before you start:** install the release candidate, either "Veal Test" from
the candidate installer or the real Veal once it is updated. "The tray" means
the Veal icon in the taskbar's notification area. It may sit under the ^
overflow arrow.

---

## 1. An agent keeps working while Veal is closed (acceptance 1 and 2)

1. Open a pane and start `claude` on something that takes over 5 minutes, such
   as "read every file in this repo and write a summary of each to notes.md".
2. Close Veal with the X. The dialog offers **Keep running**; choose it.
3. Hover over the tray icon. The tooltip should read "Veal: 1 agent running".
4. Wait 5 minutes. Click the tray icon.
5. Check that Veal opens with the same pane, the same screen and scrollback,
   and the red badge and sidebar dot already right. The agent kept working
   while the window was closed (its output moved on, or the task finished).
6. In that agent, try a prompt, Shift+Enter for a new line, pasting text,
   pasting an image, and Ctrl+C. Each one should work.

Result: **pass**

## 2. The tray

With an agent running and Veal closed with Keep running:

1. Left click the icon: Veal opens. Close it again with Keep running.
2. Right click, then **Open Veal**: Veal opens. Close it again with Keep running.
3. Right click, then **Stop everything and quit**. It asks first, because an agent is
   running. Choose No: nothing stops. Do it again and choose Yes: the icon goes
   away and the agent ends.
4. With only idle shells (no agent), close Veal. There is no dialog and no icon.
5. Does the icon look right at your display scaling? Is the tooltip text right?

Result: **pass**

## 3. Another Windows account cannot reach your shells (acceptance 14)

Needs a second local account. Sign in to it while your own account has Veal
running.

1. In PowerShell in the **second** account, run:

   ```powershell
   $p = New-Object System.IO.Pipes.NamedPipeClientStream('.', 'veal-S-1-5-21-2047368913-858741333-358495009-1001', 'InOut'); $p.Connect(3000)
   ```

2. Expected: an "Access to the path is denied" error. Not expected: the command
   returning with no error, which means it connected.

Result: **not done (no second account)**

## 4. Sleep (acceptance 16)

1. Start `claude` on a task in a pane.
2. Put the PC to sleep (Start, Power, Sleep) for at least a minute, then wake it.
3. Type into the agent. It answers, and the pane is not showing "background
   service stopped". The agent pausing during sleep is expected.

Result: **pass**

## 5. Carried over from earlier plans

1. **VS Code terminal:** run Veal from VS Code's terminal, then close that
   terminal. Veal and its panes keep working.
   Result: **pass**
2. **Console flash:** open 10 new panes one after another. Did any black
   console window flash up?
   Result: **pass**
3. **Three agents for 20 minutes (plan-15 A4):** three panes, each running
   `claude` on a task. After 20 minutes all three still respond and the window
   isn't sluggish.
   Result: **pass**
4. **Wheel scrolling (plan-15 A5):** in a `claude` pane with a long
   conversation, the mouse wheel scrolls its history smoothly.
   Result: **pass**

## 6. A clean PC

On a PC or VM that has never had Veal, with Defender on:

1. Download and run the candidate installer. Was there a SmartScreen prompt
   ("Windows protected your PC")? What did it say?
2. Open Veal and a pane. Did Defender flag, quarantine or noticeably slow down
   `veal-daemon.exe`? Look under Windows Security, then Protection history.

Result: **not done (no clean PC)**

## 7. Optional: the real update

This replaces your installed Veal. Update the installed v0.1.2 to the new
version from its own update card. Afterwards your pinned projects and panes
come back, and a `veal-daemon.exe` is running (in Task Manager, Details).

Result: **not done (optional; the real Veal is still 0.1.2)**
