// Builds a page with two real xterm.js 6.0 terminals (DOM renderer): one fed the live stream
// up to a cut, one fed only the snapshot taken at that cut. usage: node eye.mjs <frac> <name>
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const here = dirname(fileURLToPath(import.meta.url));
const [fracArg, name] = process.argv.slice(2);
const file = resolve(here, "../out/claude.bin");
const raw = readFileSync(file);
const COLS = raw.readUInt16LE(0), ROWS = raw.readUInt16LE(2);
const evs = [];
for (let i = 4; i < raw.length; ) { const n = raw.readUInt32LE(i + 1); evs.push(raw.subarray(i + 5, i + 5 + n)); i += 5 + n; }
const total = evs.reduce((a, b) => a + b.length, 0);
let want = Math.floor(total * Number(fracArg)), at = 0, ei = 0, off = 0;
for (let i = 0; i < evs.length; i++) { if (want < at + evs[i].length) { ei = i; off = want - at; break; } at += evs[i].length; }
const dir = resolve(here, `../out/eye-${name}`);
mkdirSync(dir, { recursive: true });
writeFileSync(resolve(dir, "cuts.txt"), `${ei} ${off}`);
const r = spawnSync(resolve(here, "../rs/target/release/gate.exe"), ["snap", "alacritty-tail", file, resolve(dir, "cuts.txt"), dir], { encoding: "utf8" });
if (r.status !== 0) throw new Error(r.stderr);
const stream = Buffer.concat([...evs.slice(0, ei), evs[ei].subarray(0, off)]);
const snap = readFileSync(resolve(dir, "0.snap"));
const X = resolve(here, "../../../node_modules/.pnpm/@xterm+xterm@6.0.0/node_modules/@xterm/xterm");
const xterm = readFileSync(`${X}/lib/xterm.js`, "utf8");
const css = readFileSync(`${X}/css/xterm.css`, "utf8");
const addon = readFileSync(resolve(here, "node_modules/@xterm/addon-unicode11/lib/addon-unicode11.js"), "utf8");
const html = `<!doctype html><meta charset="utf-8"><title>Restore check</title><style>${css}
body{margin:0;background:#111;color:#ddd;font:13px sans-serif}.box{display:block;margin:8px;width:760px}</style>
<div class="box"><div>control: saw the whole stream</div><div id="a" class="t"></div></div>
<div class="box"><div>restored: snapshot only</div><div id="b" class="t"></div></div>
<script>${xterm}</script><script>${addon}</script><script>
const d=(s)=>Uint8Array.from(atob(s),(c)=>c.charCodeAt(0));
function mk(id,bytes){const t=new Terminal({cols:${COLS},rows:${ROWS},fontSize:11,fontFamily:'Consolas,monospace',scrollback:5000,allowProposedApi:true,cursorBlink:false,windowsPty:{backend:'conpty',buildNumber:26200}});
t.loadAddon(new Unicode11Addon.Unicode11Addon());t.unicode.activeVersion='11';t.open(document.getElementById(id));return new Promise(r=>t.write(bytes,r));}
Promise.all([mk('a',d("${stream.toString("base64")}")),mk('b',d("${snap.toString("base64")}"))]).then(()=>{const m=document.createElement('div');m.id='done';document.body.append(m)});
</script>`;
writeFileSync(resolve(dir, `${name}.html`), html);
console.log(resolve(dir, `${name}.html`), JSON.stringify({ cutBytes: want, totalBytes: total, snapBytes: snap.length }));
