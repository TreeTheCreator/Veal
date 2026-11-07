for (let i = 1; i <= 7000; i++) {
  const c = 16 + (i % 200);
  process.stdout.write(`\x1b[38;5;${c}mline ${String(i).padStart(5, "0")}\x1b[0m \x1b[1m${"#".repeat(i % 60)}\x1b[0m\r\n`);
}
