// Drives Veal's webview over CDP (plan-19 acceptance). Start Veal with
// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>.
// node cdp.mjs <port> eval <expr> | type <pane> <text> | enter <pane> | ctrlc <pane>
//                    | shot <file> | pause <seconds>
import { writeFileSync } from "node:fs";
const [port, cmd, ...rest] = process.argv.slice(2);
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r));
let next = 1;
const waiting = new Map();
ws.addEventListener("message", (e) => {
  const m = JSON.parse(e.data);
  if (m.id && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); }
});
const send = (method, params = {}) => new Promise((r) => { const id = next++; waiting.set(id, r); ws.send(JSON.stringify({ id, method, params })); });
const evaluate = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  return r.result?.result?.value ?? r.result?.exceptionDetails?.exception?.description;
};
if (cmd === "eval") console.log(JSON.stringify(await evaluate(rest.join(" "))));
if (cmd === "type") {
  const [idx, ...text] = rest;
  await evaluate(`document.querySelectorAll('.pane-term textarea')[${idx}].focus()`);
  // Literal: Enter is `enter` and Ctrl+C is `ctrlc`, so a `\r` in a Windows
  // path stays as typed.
  await send("Input.insertText", { text: text.join(" ") });
}
if (cmd === "enter") {
  await evaluate(`document.querySelectorAll('.pane-term textarea')[${rest[0]}].focus()`);
  for (const type of ["keyDown", "keyUp"])
    await send("Input.dispatchKeyEvent", { type, key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: type === "keyDown" ? String.fromCharCode(13) : undefined });
}
if (cmd === "ctrlc") {
  await evaluate(`document.querySelectorAll('.pane-term textarea')[${rest[0]}].focus()`);
  await send("Input.insertText", { text: String.fromCharCode(3) });
}
// Freezes the page in the debugger, as a person pausing it in devtools would.
// The pause ends with this connection, so it is held here.
if (cmd === "pause") {
  await send("Debugger.enable");
  await send("Debugger.pause");
  await new Promise((r) => setTimeout(r, Number(rest[0]) * 1000));
  await send("Debugger.resume");
}
if (cmd === "shot") {
  const r = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(rest[0], Buffer.from(r.result.data, "base64"));
}
ws.close();
