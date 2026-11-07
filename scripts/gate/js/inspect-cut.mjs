// Shows one cut up close: the control and the restored screen side by side, and the
// bytes around it. usage: node inspect-cut.mjs <scenario> <event index> <offset>
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import pkg from "@xterm/headless";
const { Terminal } = pkg;
import u11 from "@xterm/addon-unicode11";
const { Unicode11Addon } = u11;

const here = dirname(fileURLToPath(import.meta.url));
const gate = resolve(here, "../rs/target/release/gate.exe");
const [name, evArg, offArg] = process.argv.slice(2);
const cutEv = Number(evArg), cutOff = Number(offArg);
const file = resolve(here, `../out/${name}.bin`);
const raw = readFileSync(file);
const COLS = raw.readUInt16LE(0), ROWS = raw.readUInt16LE(2);
const evs = [];
for (let i = 4; i < raw.length; ) {
  const k = String.fromCharCode(raw[i]), n = raw.readUInt32LE(i + 1), p = raw.subarray(i + 5, i + 5 + n);
  i += 5 + n;
  evs.push({ k, b: p });
}

const fresh = () => {
  const t = new Terminal({ cols: COLS, rows: ROWS, scrollback: 5000, allowProposedApi: true, windowsPty: { backend: "conpty", buildNumber: 26200 } });
  t.loadAddon(new Unicode11Addon());
  t.unicode.activeVersion = "11";
  return t;
};
const write = (t, b) => new Promise((r) => t.write(b, r));
const rows = (t) => {
  const b = t.buffer.active, l = [];
  for (let i = 0; i < t.rows; i++) l.push(b.getLine(b.baseY + i)?.translateToString(true) ?? "");
  return { type: b.type, cx: b.cursorX, cy: b.cursorY, lines: l };
};

const control = fresh();
const stream = [];
for (let i = 0; i <= cutEv; i++) {
  if (evs[i].k !== "D") continue;
  stream.push(i === cutEv ? evs[i].b.subarray(0, cutOff) : evs[i].b);
}
const upto = Buffer.concat(stream);
await write(control, upto);

const dir = mkdtempSync(join(tmpdir(), "gate-cut-"));
const cuts = join(dir, "cuts.txt");
writeFileSync(cuts, `${cutEv} ${cutOff}`);
const r = spawnSync(gate, ["snap", "alacritty-tail", file, cuts, dir], { encoding: "utf8" });
if (r.status !== 0) throw new Error(r.stdout + r.stderr);
const snap = readFileSync(join(dir, "0.snap"));
const restored = fresh();
await write(restored, snap);

const a = rows(control), b = rows(restored);
console.log(`control  ${a.type} cursor ${a.cx},${a.cy}`);
console.log(`restored ${b.type} cursor ${b.cx},${b.cy}`);
for (let i = 0; i < a.lines.length; i++) {
  const same = a.lines[i] === b.lines[i];
  console.log(`${String(i).padStart(2)} ${same ? " " : "A"} ${a.lines[i].slice(0, 100)}`);
  if (!same) console.log(`   B ${b.lines[i].slice(0, 100)}`);
}
const show = (buf) => buf.toString("latin1").replace(/[\x00-\x1f\x7f-\xff]/g, (c) => (c === "\x1b" ? "␛" : `\\x${c.charCodeAt(0).toString(16).padStart(2, "0")}`));
console.log("\n--- last 900 bytes before the cut\n" + show(upto.subarray(-900)));
const seqs = {};
for (const m of upto.toString("latin1").matchAll(/\x1b\[([?>=<]?)[0-9;:]*([ -\/]*[@-~])/g)) {
  const k = m[1] + m[2];
  seqs[k] = (seqs[k] ?? 0) + 1;
}
console.log("\n--- CSI finals so far\n" + JSON.stringify(seqs));
process.exit(0);
