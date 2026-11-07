# Screen restore harness (plan-19, section 0)

Tests whether a terminal emulator's snapshot restores a real session in xterm.js.
It is not part of Veal's build: `rs/` is its own crate, outside the workspace.

`rs/src/main.rs` also holds the snapshot serializer that passed the gate. Step 3 of
plan-19 moves that part into `veal-core`; the rest of this folder stays as the way
to re-run the checks.

## Setup

```
cd scripts/gate/rs && cargo build --release
cd ../js && npm install
```

## Running

Run from `js/`. Recordings go to `out/`. `record.mjs` works in `work/`.

```
node record.mjs styles          # also: scrollback flood cls shell-resize claude claude-resize
node make-direct.mjs            # a hostile stream that skips ConPTY
node make-widths.mjs            # glyphs where width tables disagree

XTERM_UNICODE=11 node compare.mjs claude alacritty-tail 150 | node summ.mjs
```

Candidates for `compare.mjs`: `alacritty-tail` (the winner), `alacritty` (no partial
sequence tracking), `vt100`, `raw`. Leave `XTERM_UNICODE` unset for Veal's current
widths, `11` for the Unicode 11 addon, `15` for the graphemes addon.

- `g5.mjs`: a snapshot landing on a used terminal. Needs a `scrollback` comparison first.
- `truth.mjs`: scores `shell-resize` against the text that was actually printed.
- `scan.mjs <names>`: counts sequences in recordings, to see if a known gap matters.
- `eye.mjs <fraction> <name>`: a page with two real xterms, to screenshot and diff.
- `../rs/target/release/gate.exe bench ../out/flood.bin 1`: the cost check.

## The `claude` scenarios

These start a real Claude Code session and send one short prompt, so they use some
of your Claude usage. They also accept the trust prompt for `work/`.
