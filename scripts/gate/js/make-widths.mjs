// Glyphs where Unicode 11 (xterm's addon) and a newer width table may disagree.
import { writeFileSync } from "node:fs";
const rows = [
  "claude glyphs: ⏺ ✻ ✽ ✶ ✢ · ⎿ ❯ ⏵⏵ ◐ ─ │ ╭ ╮ ╰ ╯ end",
  "older emoji: 🦊 🚀 ✅ ❌ ⚠️ 🔥 end",
  "newer emoji: 🥲 🫠 🪼 🫡 end",
  "zwj family: 👨‍👩‍👧 flag: 🇯🇵 skin: 👍🏽 end",
  "cjk: 日本語 한국어 中文 end",
];
const s = rows.map((r) => r + "\r\n").join("");
const b = Buffer.from(s, "utf8");
const e = Buffer.alloc(5); e[0] = 68; e.writeUInt32LE(b.length, 1);
writeFileSync(new URL("../out/widths.bin", import.meta.url), Buffer.concat([Buffer.from(new Uint16Array([120, 30]).buffer), e, b]));
