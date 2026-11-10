// node old-session.mjs <pipe>, the daemon pipe: veal-<SID> under \\.\pipe
// Speaks protocol 99 to the daemon on argv[2] and starts one session, as an
// older Veal would.
import net from "node:net";
const pipe = process.argv[2];
const frame = (obj) => { const body = Buffer.from(JSON.stringify(obj)); const h = Buffer.alloc(5); h.writeUInt32LE(body.length + 1, 0); h[4] = 1; return Buffer.concat([h, body]); };
const s = net.connect(pipe, () => {
  s.write(frame({ id: 1, t: "hello", protocol: 99, build: "old", role: "desktop" }));
  s.write(frame({ id: 2, t: "spawn", key: "old-session", cwd: process.env.USERPROFILE, cols: 80, rows: 24, shell: "cmd.exe", proxy_mode: "off", proxy_port: null, proxy_bypass: null, env: null, responder: true, controller: true }));
});
let seen = "";
s.on("data", (d) => { seen += d.toString("latin1"); if (seen.includes('"id":2')) { console.log(seen.includes('"opened"') ? "session started" : "spawn reply: " + seen.slice(-200)); s.end(); } });
s.on("error", (e) => { console.error(e.message); process.exit(1); });
