// Records a scenario through a real ConPTY. A headless xterm 6.0 is the responder,
// as xterm is in Veal, so programs see the same answers they would in a pane.
// usage: node record.mjs <scenario>
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import pkg from "@xterm/headless";
const { Terminal } = pkg;

const here = dirname(fileURLToPath(import.meta.url));
const gate = resolve(here, "../rs/target/release/gate.exe");
const out = resolve(here, "../out");
const work = resolve(here, "../work");
mkdirSync(out, { recursive: true });
mkdirSync(work, { recursive: true });
const name = process.argv[2];
const COLS = 120, ROWS = 30;

const term = new Terminal({ cols: COLS, rows: ROWS, scrollback: 5000, allowProposedApi: true, windowsPty: { backend: "conpty", buildNumber: 26200 } });
const env = { ...process.env };
for (const k of Object.keys(env)) if (k === "CLAUDECODE" || k.startsWith("CLAUDE_CODE_") ) delete env[k];
const shell = "powershell.exe";
const child = spawn(gate, ["pty", String(COLS), String(ROWS), work, shell, "-NoLogo", "-NoProfile"], { env, stdio: ["pipe", "pipe", "inherit"] });

const events = [];
let lastData = Date.now(), exited = false, total = 0;
let buf = "";
child.stdout.on("data", (chunk) => {
  buf += chunk.toString("latin1");
  let i;
  while ((i = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, i).trim();
    buf = buf.slice(i + 1);
    if (line[0] === "D") {
      const b = Buffer.from(line.slice(1), "base64");
      events.push(["D", b]);
      total += b.length;
      lastData = Date.now();
      term.write(b);
    } else if (line[0] === "X") exited = true;
  }
});
term.onData((d) => send(d));
function send(s) { child.stdin.write("W" + Buffer.from(s, "utf8").toString("base64") + "\n"); }
function resize(c, r) {
  events.push(["R", Buffer.from(new Uint16Array([c, r]).buffer)]);
  term.resize(c, r);
  child.stdin.write(`R${c} ${r}\n`);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function screen() {
  const b = term.buffer.active, l = [];
  for (let i = 0; i < term.rows; i++) l.push(b.getLine(b.baseY + i)?.translateToString(true) ?? "");
  return l.join("\n");
}
async function until(pred, ms, what) {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (pred(screen())) return true; await sleep(100); }
  console.log(`--- timed out waiting for ${what}\n${screen()}\n---`);
  return false;
}
async function idle(ms, max = 240000) {
  const end = Date.now() + max;
  while (Date.now() < end && Date.now() - lastData < ms) await sleep(100);
}
const type = async (s) => { for (const ch of s) { send(ch); await sleep(8); } };
const runNode = (file) => send(`node "${resolve(here, file)}"\r`);

const scenarios = {
  // Styles, wide glyphs and modes through ConPTY.
  async styles() {
    await idle(1500);
    runNode("emit-styles.mjs");
    await idle(2500);
  },
  // 7000 coloured numbered lines: more than the 5000 kept.
  async scrollback() {
    await idle(1500);
    runNode("emit-lines.mjs");
    await idle(4000);
  },
  // Heavy SGR output for the cost measurement.
  async flood() {
    await idle(1500);
    runNode("emit-flood.mjs");
    await idle(4000, 180000);
  },
  // A plain shell on the main screen, with soft-wrapped output, resized repeatedly.
  async "shell-resize"() {
    await idle(1500);
    runNode("emit-wrap.mjs");
    await idle(2500);
    for (const [c, r] of [[80, 24], [140, 40], [60, 20], [120, 30], [100, 30]]) {
      resize(c, r);
      await idle(1500);
      send("echo resized-" + c + "x" + r + "\r");
      await idle(1200);
    }
    runNode("emit-wrap.mjs");
    await idle(2500);
  },
  async cls() {
    await idle(1500);
    runNode("emit-wrap.mjs");
    await idle(2000);
    send("cls\r");
    await idle(1500);
    send("echo after-cls\r");
    await idle(1500);
  },
  // Real agent, idle and redrawing.
  async claude() { await claudeRun(false); },
  // Real agent with resizes during its output.
  async "claude-resize"() { await claudeRun(true); },
};

async function claudeRun(doResize) {
  await idle(1500);
  send("claude\r");
  const ready = (s) => /trust|\? for shortcuts|Try "|bypass|Welcome|shift\+tab/i.test(s);
  await until(ready, 45000, "claude start");
  await idle(1500);
  if (/Yes, I trust/i.test(screen())) { send("\x1b[B"); await sleep(500); send("\r"); await idle(4000); }
  console.log("--- started\n" + screen());
  await sleep(1500);
  { const b = term.buffer.active; const last = b.getLine(b.baseY + b.cursorY)?.translateToString(true) ?? "";
    if (/^PS /.test(last)) { console.log("ABORT: claude is not running, not typing the prompt"); child.kill(); process.exit(2); } }
  await type("Without using any tools, write a numbered list of 45 one-line facts about terminal emulators, then stop.");
  await sleep(300);
  send("\r");
  await sleep(5000);
  if (doResize) {
    const steps = [[100, 30], [140, 40], [80, 24], [120, 30]];
    for (const [c, r] of steps) { await sleep(5000); resize(c, r); }
  }
  await idle(9000, 180000);
  console.log("--- answered\n" + screen());
  await type("/exit");
  await sleep(300);
  send("\r");
  await idle(3000, 20000);
}

await sleep(200);
await scenarios[name]();
send("exit\r");
const end = Date.now() + 5000;
while (!exited && Date.now() < end) await sleep(50);
child.kill();

// events.bin: [u16 cols][u16 rows] then [u8 kind][u32 len][payload]
const parts = [Buffer.from(new Uint16Array([COLS, ROWS]).buffer)];
for (const [k, p] of events) {
  const h = Buffer.alloc(5);
  h[0] = k.charCodeAt(0);
  h.writeUInt32LE(p.length, 1);
  parts.push(h, p);
}
const f = join(out, `${name}.bin`);
writeFileSync(f, Buffer.concat(parts));
const all = Buffer.concat(events.filter((e) => e[0] === "D").map((e) => e[1])).toString("latin1");
const count = (re) => (all.match(re) || []).length;
console.log(JSON.stringify({ scenario: name, bytes: total, events: events.length, resizes: events.filter((e) => e[0] === "R").length,
  seq: { dsr6n: count(/\x1b\[6n/g), da: count(/\x1b\[>?0?c/g), alt1049: count(/\x1b\[\?1049[hl]/g), decstbm: count(/\x1b\[\d*;?\d*r/g), ed2: count(/\x1b\[2J/g), ed3: count(/\x1b\[3J/g), sync2026: count(/\x1b\[\?2026[hl]/g), osc8: count(/\x1b\]8;/g), blink: count(/\x1b\[[0-9;]*\b5m/g), overline: count(/\x1b\[[0-9;]*\b53m/g), bpaste: count(/\x1b\[\?2004[hl]/g), focus: count(/\x1b\[\?1004[hl]/g), kitty: count(/\x1b\[[>?<=][0-9;]*u/g) } }));
process.exit(0);
