// A hand-built stream that skips ConPTY: everything a program could send raw.
// Each part is its own Data event, so cuts land between and inside them.
import { writeFileSync } from "node:fs";
const parts = [];
const w = (s) => parts.push(Buffer.from(s, "utf8"));
w("\x1b]0;direct title\x07");
for (let i = 0; i < 40; i++) w(`\x1b[38;5;${i + 20}mhistory ${i}\x1b[0m\r\n`);
w("\x1b[1;4;31mstyled\x1b[0m \x1b[4:3;58;5;196mcurly\x1b[0m \x1b[4:2mdouble\x1b[0m\r\n");
w("\x1b[44m bce line\x1b[K\x1b[0m\r\n");
w("\x1b]8;id=a;https://example.com\x1b\\a link\x1b]8;;\x1b\\\r\n");
w("y".repeat(119)); w("z"); // pending wrap at the last column
w("\r\nafter wrap\r\n");
w("q".repeat(200) + "\r\n"); // a soft-wrapped line
w("\x1b[?2004h"); w("\x1b[?1004h"); w("\x1b[?1h"); w("\x1b="); w("\x1b[?1002h\x1b[?1006h");
w("\x1b[6 q"); w("\x1b[?25l"); w("\x1b[4h"); w("\x1b[4l"); w("\x1b[?25h");
w("\x1b[5;5H\x1b7\x1b[20;1H"); // DECSC at 5,5
w("saved-cursor set\r\n");
w("\x1b[33m"); w("pen left yellow "); // the current pen carries over
w("\x1b[?1049h"); w("\x1b[H\x1b[32malt line one\x1b[0m\r\nalt line two"); w("\x1b[12;40Hmiddle");
w("\x1b[?7l"); w("n".repeat(130)); w("\x1b[?7h");
w("\x1b[?1049l"); w("back, cursor restored\r\n");
w("\x1b8"); w("X at saved\r\n");
w("\x1b[20;1H");
// Things the emulator may not keep. Reported, not assumed.
w("\x1b[5mblink\x1b[0m \x1b[53moverline\x1b[0m\r\n");
w("\x1b[3;20r"); w("region set\r\n"); w("\x1b[r");
w("\x1b[2J"); w("\x1b[Hcleared with ED2\r\n");
w("\x1b[3J"); w("cleared scrollback\r\n");
const h = Buffer.from(new Uint16Array([120, 30]).buffer);
const out = [h];
for (const p of parts) { const e = Buffer.alloc(5); e[0] = 68; e.writeUInt32LE(p.length, 1); out.push(e, p); }
writeFileSync(new URL("../out/direct.bin", import.meta.url), Buffer.concat(out));
console.log("parts", parts.length);
