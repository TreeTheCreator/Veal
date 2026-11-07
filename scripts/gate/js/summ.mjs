// Prints one summary line, then per-kind counts and the first samples.
let s = "";
process.stdin.on("data", (d) => (s += d)).on("end", () => {
  const r = JSON.parse(s);
  console.log(JSON.stringify({ scenario: r.scenario, cand: r.candidate, bytes: r.bytes, resizes: r.resizes, cuts: r.cuts, exactAtCut: r.exactAtCut, exactAtEnd: r.exactAtEnd, snapReplies: r.snapReplyTotal, extraReplyCuts: r.extraReplyCuts, snap: r.snapStats }));
  console.log("  kinds " + JSON.stringify(r.kinds));
  console.log("  " + JSON.stringify({ visibleByDistance: r.visibleByDistance, textLostAtCut: r.textLostAtCut, textLostAtEnd: r.textLostAtEnd, wrapOnlyAtEnd: r.wrapOnlyAtEnd }));
  for (const l of r.lostSamples ?? []) console.log("  lost " + JSON.stringify(l).slice(0, 400));
  const n = Number(process.argv[2] ?? 3);
  const width = Number(process.argv[3] ?? 420);
  for (const x of r.samples.slice(0, n)) console.log("  " + JSON.stringify(x).slice(0, width));
});
