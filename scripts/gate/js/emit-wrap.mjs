for (let i = 1; i <= 60; i++) {
  const n = 20 + ((i * 37) % 260);
  process.stdout.write(`\x1b[38;5;${16 + (i % 200)}m${String(i).padStart(3, "0")}:\x1b[0m ${"abcdefghij".repeat(30).slice(0, n)}\r\n`);
}
