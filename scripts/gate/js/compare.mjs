// usage: node compare.mjs <scenario|file.bin> <candidate> [cuts=60] [seed=1]
// Candidates: alacritty-tail (veal-core, what ships), vt100, raw (ring-buffer replay baseline).
// For each cut: restore a snapshot into a fresh xterm, compare to a control that saw
// the whole stream so far, then feed both the rest and compare again.
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import pkg from "@xterm/headless";
const { Terminal } = pkg;
import u11 from "@xterm/addon-unicode11";
const { Unicode11Addon } = u11;
import ug from "@xterm/addon-unicode-graphemes";
const UG = ug.UnicodeGraphemesAddon;
const U11 = process.env.XTERM_UNICODE === "11";

const here = dirname(fileURLToPath(import.meta.url));
const gate = resolve(here, "../rs/target/release/gate.exe");
const [name, cand, cutsArg, seedArg] = process.argv.slice(2);
const N = Number(cutsArg ?? 60);
let seed = Number(seedArg ?? 1);
const rand = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 2 ** 32);
const file = resolve(here, `../out/${name}.bin`);

const raw = readFileSync(file);
const COLS = raw.readUInt16LE(0), ROWS = raw.readUInt16LE(2);
const evs = [];
for (let i = 4; i < raw.length; ) {
  const k = String.fromCharCode(raw[i]), n = raw.readUInt32LE(i + 1), p = raw.subarray(i + 5, i + 5 + n);
  i += 5 + n;
  evs.push(k === "D" ? { k, b: p } : { k, c: p.readUInt16LE(0), r: p.readUInt16LE(2) });
}
const dataIdx = evs.map((e, i) => (e.k === "D" ? i : -1)).filter((i) => i >= 0);
const totalBytes = dataIdx.reduce((a, i) => a + evs[i].b.length, 0);

// Random byte offsets, so cuts land anywhere, including mid-sequence; plus just after each resize.
const cuts = [];
for (let n = 0; n < N; n++) {
  let off = Math.floor(rand() * totalBytes), at = 0;
  for (const i of dataIdx) {
    if (off < at + evs[i].b.length) { cuts.push([i, off - at]); break; }
    at += evs[i].b.length;
  }
}
evs.forEach((e, i) => {
  if (e.k !== "R") return;
  const nextData = dataIdx.find((d) => d > i);
  if (nextData !== undefined) cuts.push([nextData, 0]);
  for (const d of dataIdx.filter((d) => d > i).slice(1, 3)) cuts.push([d, 0]);
});
cuts.sort((a, b) => a[0] - b[0] || a[1] - b[1]);

const opts = (c, r) => ({ cols: c, rows: r, scrollback: 5000, allowProposedApi: true, windowsPty: { backend: "conpty", buildNumber: 26200 } });
const write = (t, b) => new Promise((res) => t.write(b, res));
async function feed(t, from, to) {
  let batch = [];
  const flush = async () => { if (batch.length) { await write(t, Buffer.concat(batch)); batch = []; } };
  for (let i = from; i < to; i++) { const e = evs[i]; if (e.k === "D") batch.push(e.b); else { await flush(); t.resize(e.c, e.r); } }
  await flush();
}

const outdir = resolve(here, `../out/snap-${name}-${cand}`);
rmSync(outdir, { recursive: true, force: true });
mkdirSync(outdir, { recursive: true });
let snapStats = null;
if (cand !== "raw") {
  const cutFile = resolve(outdir, "cuts.txt");
  writeFileSync(cutFile, cuts.map((c) => c.join(" ")).join("\n"));
  const r = spawnSync(gate, ["snap", cand, file, cutFile, outdir], { encoding: "utf8", maxBuffer: 1 << 28 });
  if (r.status !== 0) { console.log(r.stdout, r.stderr); process.exit(1); }
  snapStats = JSON.parse(r.stdout.trim());
}

let __link = null;
function __uri(c) { const id = c.extended?.urlId ?? 0; return id && __link ? (__link.getLinkData(id)?.uri ?? "?") : ""; }
function cellKey(c) {
  const ch = c.getChars() === "" ? " " : c.getChars();
  if (c.getWidth() === 0) return "\0spacer";
  const parts = [ch, c.getWidth(), c.isBlink() ? 1 : 0, c.isOverline() ? 1 : 0, c.extended?.underlineStyle ?? 0, __uri(c)];
  parts.push(c.isBold() ? 1 : 0, c.isDim() ? 1 : 0, c.isItalic() ? 1 : 0, c.isUnderline() ? 1 : 0, c.isInverse() ? 1 : 0, c.isInvisible() ? 1 : 0, c.isStrikethrough() ? 1 : 0);
  const invisibleFg = ch === " " && !c.isUnderline() && !c.isStrikethrough() && !c.isInverse();
  parts.push(invisibleFg ? 0 : c.getFgColorMode(), invisibleFg ? 0 : c.getFgColor(), c.getBgColorMode(), c.getBgColor());
  return parts.join(",");
}
const blinkOver = (c) => (c.isBlink() ? "B" : "") + (c.isOverline() ? "O" : "");

function compareBuf(a, b, cols, out, label, ta, tb) {
  const __rows = ta.rows;
  const lenA = a.length, lenB = b.length;
  if (lenA !== lenB) out.push({ kind: `${label}.length`, a: lenA, b: lenB });
  const n = Math.min(lenA, lenB);
  let cellBad = 0, textBad = 0, first = null, visibleBad = 0;
  const ca = a.getNullCell(), cb = b.getNullCell();
  for (let i = 1; i <= n; i++) { // aligned at the bottom
    const la = a.getLine(lenA - i), lb = b.getLine(lenB - i);
    if (!la || !lb) continue;
    if (la.translateToString(true, 0, cols).trimEnd() !== lb.translateToString(true, 0, cols).trimEnd()) {
      textBad++; if (i <= __rows) visibleBad++;
      if (!first) first = { from_bottom: i, a: la.translateToString(true).slice(0, 110), b: lb.translateToString(true).slice(0, 110) };
    }
    for (let x = 0; x < cols; x++) {
      la.getCell(x, ca); lb.getCell(x, cb);
      __link = ta._core._oscLinkService; const ka = cellKey(ca); __link = tb._core._oscLinkService; const kb = cellKey(cb);
      if (ka !== kb) {
        cellBad++; if (i <= __rows) visibleBad++;
        if (!first) first = { from_bottom: i, x, a: ka, b: kb };
      }
    }
  }
  if (cellBad || textBad) out.push({ kind: visibleBad ? `${label}.VISIBLE` : `${label}.scrollback`, cellBad, textBad, visibleBad, first });
}

function logical(t) {
  const b = t.buffer.normal, out = [];
  for (let i = 0; i < b.length; i++) {
    const l = b.getLine(i), text = l.translateToString(true, 0, t.cols);
    if (l.isWrapped && out.length) out[out.length - 1] += text;
    else out.push(text);
  }
  return out.map((x) => x.trimEnd()).filter((x) => x.length);
}
function state(t) {
  const core = t._core, dp = core.coreService.decPrivateModes, m = t.modes;
  return {
    type: t.buffer.active.type, cx: t.buffer.active.cursorX, cy: t.buffer.active.cursorY,
    hidden: core.coreService.isCursorHidden, style: dp.cursorStyle ?? null, blink: dp.cursorBlink ?? null,
    appCursor: m.applicationCursorKeysMode, appKeypad: m.applicationKeypadMode, bracketed: m.bracketedPasteMode,
    insert: m.insertMode, mouse: m.mouseTrackingMode, origin: m.originMode, focus: m.sendFocusMode, wrap: m.wraparoundMode,
    title: t.__title ?? null, scrollTop: core.buffer.scrollTop, scrollBottom: core.buffer.scrollBottom, savedX: core.buffers.normal.savedX, savedY: Math.max(core.buffers.normal.savedY - core.buffers.normal.ybase, 0),
  };
}
function mk(c, r) {
  const t = new Terminal(opts(c, r));
  if (U11) { t.loadAddon(new Unicode11Addon()); t.unicode.activeVersion = "11"; }
  if (process.env.XTERM_UNICODE === "15") { t.loadAddon(new UG()); t.unicode.activeVersion = "15-graphemes"; }
  t.__replies = [];
  t.__title = null;
  t.onData((d) => t.__replies.push(d));
  t.onTitleChange((s) => (t.__title = s));
  return t;
}
function compare(ctrl, rest, out, label) {
  const sa = state(ctrl), sb = state(rest);
  for (const k of Object.keys(sa)) if (sa[k] !== sb[k]) out.push({ kind: `${label}.state.${k}`, a: sa[k], b: sb[k] });
  compareBuf(ctrl.buffer.normal, rest.buffer.normal, ctrl.cols, out, `${label}.normal`, ctrl, rest);
  if (sa.type === "alternate") compareBuf(ctrl.buffer.alternate, rest.buffer.alternate, ctrl.cols, out, `${label}.alt`, ctrl, rest);
}

// The stream up to the cut, for the raw baseline (a 1 MiB ring, as OUTBOX_CAP).
const RING = 1024 * 1024;
const lastResizeBefore = (ei) => { for (let i = ei; i >= 0; i--) if (evs[i].k === "R") return i; return -1; };
const visibleByDistance = {};
let textLostAtEnd = 0, textLostAtCut = 0, wrapOnlyAtEnd = 0;
const lostSamples = [];
function textDiff(x, y) {
  const c = new Map();
  for (const l of x) c.set(l, (c.get(l) ?? 0) + 1);
  for (const l of y) c.set(l, (c.get(l) ?? 0) - 1);
  const missing = [], extra = [];
  for (const [l, n] of c) { if (n > 0) missing.push(l); if (n < 0) extra.push(l); }
  return { missing, extra };
}
const results = { cuts: cuts.length, exactAtCut: 0, exactAtEnd: 0, snapReplyCuts: 0, snapReplyTotal: 0, kinds: {}, samples: [], extraReplyCuts: 0 };
const bump = (k) => (results.kinds[k] = (results.kinds[k] ?? 0) + 1);

// Control runs alongside. To avoid quadratic re-feeds the end state is computed once.
const endCtrl = mk(COLS, ROWS);
await feed(endCtrl, 0, evs.length);
const ctrlEndReplies = endCtrl.__replies.length;

const ctrl = mk(COLS, ROWS);
let pos = 0, consumed = 0;
for (let ci = 0; ci < cuts.length; ci++) {
  const [ei, off] = cuts[ci];
  if (pos < ei) { if (consumed) { await write(ctrl, evs[pos].b.subarray(consumed)); consumed = 0; pos++; } await feed(ctrl, pos, ei); pos = ei; }
  if (off > consumed) { await write(ctrl, evs[ei].b.subarray(consumed, off)); consumed = off; }
  const replyAtCut = ctrl.__replies.length;

  const rest = mk(ctrl.cols, ctrl.rows);
  let snap;
  if (cand === "raw") {
    const chunks = [];
    for (let i = 0; i < ei; i++) if (evs[i].k === "D") chunks.push(evs[i].b);
    chunks.push(evs[ei].b.subarray(0, off));
    const all = Buffer.concat(chunks);
    snap = Buffer.concat([Buffer.from("\x1bc"), all.subarray(Math.max(0, all.length - RING))]);
  } else snap = readFileSync(`${outdir}/${ci}.snap`);
  await write(rest, snap);
  if (rest.__replies.length) { results.snapReplyCuts++; results.snapReplyTotal += rest.__replies.length; }
  rest.__replies.length = 0;

  const atCut = [];
  compare(ctrl, rest, atCut, "cut");
  if (!atCut.length) results.exactAtCut++;
  for (const m of atCut) bump(m.kind);
  if (atCut.some((m) => m.kind === "cut.normal.VISIBLE" || m.kind === "cut.alt.VISIBLE")) {
    const r = lastResizeBefore(ei);
    const key = r < 0 ? "no-resize" : ei - r <= 3 ? "within 3 events of a resize" : ei - r <= 15 ? "within 15 events" : "later";
    visibleByDistance[key] = (visibleByDistance[key] ?? 0) + 1;
  }
  { const d = textDiff(logical(ctrl), logical(rest)); if (d.missing.length || d.extra.length) textLostAtCut++; }

  // Continuation: the rest of the stream reaches both.
  await write(rest, evs[ei].b.subarray(off));
  await feed(rest, ei + 1, evs.length);
  const atEnd = [];
  compare(endCtrl, rest, atEnd, "end");
  if (!atEnd.length) results.exactAtEnd++;
  for (const m of atEnd) bump(m.kind);
  { const d = textDiff(logical(endCtrl), logical(rest));
    if (d.missing.length || d.extra.length) { textLostAtEnd++; if (lostSamples.length < 3) lostSamples.push({ cut: ci, missing: d.missing.slice(0, 3).map((x) => x.slice(0, 90)), extra: d.extra.slice(0, 3).map((x) => x.slice(0, 90)) }); }
    else if (atEnd.some((m) => m.kind.includes("normal"))) wrapOnlyAtEnd++; }
  if (rest.__replies.length !== ctrlEndReplies - replyAtCut) results.extraReplyCuts++;
  if ((atCut.length || atEnd.length) && results.samples.length < 6) results.samples.push({ cut: ci, ev: ei, off, atCut: atCut.slice(0, 4), atEnd: atEnd.slice(0, 4) });
  rest.dispose();
}
console.log(JSON.stringify({ scenario: name, candidate: cand, bytes: totalBytes, resizes: evs.filter((e) => e.k === "R").length, snapStats, visibleByDistance, textLostAtCut, textLostAtEnd, wrapOnlyAtEnd, lostSamples, ...results }, null, 1));
process.exit(0);
