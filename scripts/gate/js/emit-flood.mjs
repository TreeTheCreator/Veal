const chunk = [];
for (let i = 0; i < 60000; i++) {
  chunk.push(`\x1b[38;2;${i % 255};${(i * 7) % 255};90m${String(i).padStart(7, "0")}\x1b[0m \x1b[48;5;${i % 255}m${"word ".repeat(1 + (i % 16))}\x1b[0m\r\n`);
  if (chunk.length === 200) { process.stdout.write(chunk.join("")); chunk.length = 0; }
}
process.stdout.write(chunk.join(""));
