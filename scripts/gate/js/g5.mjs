// G5: does a snapshot's leading ESC c fully replace a terminal that is already in use?
// A resync lands on a live xterm, not a fresh one, sometimes mid-sequence.
import { readFileSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import pkg from "@xterm/headless";
const { Terminal } = pkg;
const here = dirname(fileURLToPath(import.meta.url));
const dir = resolve(here, "../out/snap-scrollback-alacritty-tail");
const snaps = readdirSync(dir).filter((f) => f.endsWith(".snap")).sort((a, b) => parseInt(a) - parseInt(b));
const snap = readFileSync(resolve(dir, snaps[snaps.length - 1]));
const opts = { cols: 120, rows: 30, scrollback: 5000, allowProposedApi: true, windowsPty: { backend: "conpty", buildNumber: 26200 } };
const write = (t, b) => new Promise((r) => t.write(b, r));
const dump = (t) => {
  const b = t.buffer.normal, l = [];
  for (let i = 0; i < b.length; i++) l.push(b.getLine(i).translateToString(true).trimEnd());
  return l.join("\n");
};
const state = (t) => JSON.stringify({ type: t.buffer.active.type, cx: t.buffer.active.cursorX, cy: t.buffer.active.cursorY, m: t.modes, hidden: t._core.coreService.isCursorHidden, top: t._core.buffer.scrollTop, bot: t._core.buffer.scrollBottom });

const fresh = new Terminal(opts);
await write(fresh, snap);
const want = dump(fresh), wantState = state(fresh);

const dirty = "\x1b[?2004h\x1b[?1000h\x1b[?1h\x1b[?25l\x1b[3;9r" + Array.from({ length: 6000 }, (_, i) => `old ${i}\r\n`).join("");
const cases = {
  "used, main screen": dirty,
  "used, in alternate screen": dirty + "\x1b[?1049h\x1b[Halt content",
  "mid CSI": dirty + "\x1b[38;5",
  "mid OSC": dirty + "\x1b]0;unfinished title",
  "mid DCS": dirty + "\x1bP1$r",
  "mid ESC": dirty + "\x1b",
  "mid UTF-8": Buffer.concat([Buffer.from(dirty), Buffer.from([0xe2, 0x94])]),
};
for (const [name, pre] of Object.entries(cases)) {
  const t = new Terminal(opts);
  const replies = [];
  t.onData((d) => replies.push(d));
  await write(t, pre);
  await write(t, snap);
  const text = dump(t) === want, st = state(t) === wantState;
  console.log(JSON.stringify({ case: name, textMatches: text, stateMatches: st, lengthWant: fresh.buffer.normal.length, lengthGot: t.buffer.normal.length, replies: replies.length }));
  if (!st) console.log("  want " + wantState + "\n  got  " + state(t));
  t.dispose();
}
process.exit(0);
