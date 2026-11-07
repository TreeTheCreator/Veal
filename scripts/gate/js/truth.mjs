// shell-resize printed known text, so each logical line can be checked against what was
// actually written. Scores the control xterm and a restored xterm, to see who lost what.
import { readFileSync, readdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import pkg from "@xterm/headless";
import u11 from "@xterm/addon-unicode11";
const { Terminal } = pkg;
const { Unicode11Addon } = u11;
const here = dirname(fileURLToPath(import.meta.url));
const gate = resolve(here, "../rs/target/release/gate.exe");
const file = resolve(here, "../out/shell-resize.bin");
const raw = readFileSync(file);
const COLS = raw.readUInt16LE(0), ROWS = raw.readUInt16LE(2);
const evs = [];
for (let i = 4; i < raw.length; ) {
  const k = String.fromCharCode(raw[i]), n = raw.readUInt32LE(i + 1), p = raw.subarray(i + 5, i + 5 + n);
  i += 5 + n;
  evs.push(k === "D" ? { k, b: p } : { k, c: p.readUInt16LE(0), r: p.readUInt16LE(2) });
}
const mk = (c, r) => { const t = new Terminal({ cols: c, rows: r, scrollback: 5000, allowProposedApi: true, windowsPty: { backend: "conpty", buildNumber: 26200 } }); t.loadAddon(new Unicode11Addon()); t.unicode.activeVersion = "11"; return t; };
const write = (t, b) => new Promise((r) => t.write(b, r));
const expected = new Map();
for (let i = 1; i <= 60; i++) { const n = 20 + ((i * 37) % 260); expected.set(String(i).padStart(3, "0"), `${String(i).padStart(3, "0")}: ${"abcdefghij".repeat(30).slice(0, n)}`); }
function logical(t) {
  const b = t.buffer.normal, out = [];
  for (let i = 0; i < b.length; i++) {
    const l = b.getLine(i), text = l.translateToString(true, 0, t.cols);
    if (l.isWrapped && out.length) out[out.length - 1] += text; else out.push(text);
  }
  return out.map((x) => x.trimEnd());
}
function which(lines, label) {
  const rows = lines.filter((l) => /^\d{3}: /.test(l));
  const seen = new Map();
  let ok = 0;
  const wrong = [];
  for (const l of rows) { const k = l.slice(0, 3); seen.set(k, (seen.get(k) ?? 0) + 1); if (l === expected.get(k)) ok++; else wrong.push(k + " len " + l.length + " want " + expected.get(k)?.length); }
  const short = [];
  for (const k of expected.keys()) if ((seen.get(k) ?? 0) < 2) short.push(k);
  console.log(JSON.stringify({ label, numbered: rows.length, correct: ok, seenOnceOrLess: short, wrong }));
}
function score(t, label) {
  if (globalThis.__final) which(logical(t), label + " detail");
  const rows = logical(t).filter((l) => /^\d{3}: /.test(l));
  let ok = 0, bad = 0, sample = null;
  for (const l of rows) {
    const want = expected.get(l.slice(0, 3));
    if (l === want) ok++;
    else { bad++; sample ??= { got: l.length, want: want?.length, tail: l.slice(-24) }; }
  }
  console.log(JSON.stringify({ label, numberedLines: rows.length, correct: ok, wrong: bad, sample }));
}
// Run a candidate that resizes exactly as the stream does; snapshot at the very end.
const outdir = resolve(here, "../out/snap-truth");
import("node:fs").then(async ({ mkdirSync, writeFileSync }) => {
  mkdirSync(outdir, { recursive: true });
  // Cut points: after everything, and just before each resize.
  const cuts = [];
  evs.forEach((e, i) => { if (e.k === "R") cuts.push([i, 0]); });
  cuts.push([evs.length, 0]);
  writeFileSync(resolve(outdir, "cuts.txt"), cuts.map((c) => c.join(" ")).join("\n"));
  const r = spawnSync(gate, ["snap", "alacritty-tail", file, resolve(outdir, "cuts.txt"), outdir], { encoding: "utf8" });
  if (r.status !== 0) { console.log(r.stderr); process.exit(1); }
  const ctrl = mk(COLS, ROWS);
  let cut = 0;
  for (let i = 0; i <= evs.length; i++) {
    if (cut < cuts.length && cuts[cut][0] === i) {
      const rest = mk(ctrl.cols, ctrl.rows);
      await write(rest, readFileSync(resolve(outdir, `${cut}.snap`)));
      globalThis.__final = i === evs.length;
      console.log(`-- ${i === evs.length ? "end of stream" : "before resize " + (cut + 1)} at ${ctrl.cols}x${ctrl.rows}`);
      score(ctrl, "control");
      score(rest, "restored");
      rest.dispose();
      if (globalThis.__final) {
        const d = spawnSync(gate, ["dump", file], { encoding: "utf8", maxBuffer: 1 << 26 });
        which(d.stdout.split(/\r?\n/), "emulator grid, no serializer");
      }
      cut++;
    }
    if (i === evs.length) break;
    const e = evs[i];
    if (e.k === "D") await write(ctrl, e.b); else ctrl.resize(e.c, e.r);
  }
  process.exit(0);
});
