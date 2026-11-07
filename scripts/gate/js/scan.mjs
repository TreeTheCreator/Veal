// How often do the known gaps occur in real streams? Parses SGR properly, so 38;5;5m is not "blink".
import { readFileSync } from "node:fs";
for (const name of process.argv.slice(2)) {
  const raw = readFileSync(new URL(`../out/${name}.bin`, import.meta.url));
  const parts = [];
  for (let i = 4; i < raw.length; ) { const n = raw.readUInt32LE(i + 1); if (raw[i] === 68) parts.push(raw.subarray(i + 5, i + 5 + n)); i += 5 + n; }
  const s = Buffer.concat(parts).toString("latin1");
  let blink = 0, overline = 0, sgr = 0;
  for (const m of s.matchAll(/\x1b\[([0-9;:]*)m/g)) {
    sgr++;
    const p = m[1].split(";").map(Number);
    for (let i = 0; i < p.length; i++) {
      if (p[i] === 38 || p[i] === 48 || p[i] === 58) { i += p[i + 1] === 5 ? 2 : p[i + 1] === 2 ? 4 : 0; continue; }
      if (p[i] === 5 || p[i] === 6) blink++;
      if (p[i] === 53) overline++;
    }
  }
  const c = (re) => (s.match(re) || []).length;
  console.log(JSON.stringify({ name, bytes: s.length, sgr, blink, overline, decstbm: c(/\x1b\[\d*;?\d*r/g), decsc: c(/\x1b7/g), decrc: c(/\x1b8/g), scosc: c(/\x1b\[s/g), ed2: c(/\x1b\[2J/g), ed3: c(/\x1b\[3J/g), osc8: c(/\x1b\]8;/g), alt: c(/\x1b\[\?(1049|47|1047)[hl]/g), dsr: c(/\x1b\[\??6n/g), da: c(/\x1b\[[>=]?0?c/g), decrqm: c(/\x1b\[\??\d+\$p/g), oscColor: c(/\x1b\]1[01];\?/g), kittyQuery: c(/\x1b\[\?u/g), kittyPush: c(/\x1b\[>\d*u/g) }));
}
