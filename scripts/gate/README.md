# Screen restore harness (plan-19, section 0)

Tests whether a terminal emulator's snapshot restores a real session in xterm.js.
It is not part of Veal's build: `rs/` is its own crate, outside the workspace.

The serializer that passed the gate now lives in `veal-core` (`session.rs`), and the
`alacritty-tail` candidate drives that code, so a run here checks what ships.

## Setup

```
cd scripts/gate/rs && cargo build --release
cd ../js && npm install
```

## Running

Run from `js/`. Recordings go to `out/`. `record.mjs` works in `work/`.

```
node record.mjs styles          # also: scrollback flood cls shell-resize claude claude-resize claude-long
node make-direct.mjs            # a hostile stream that skips ConPTY
node make-widths.mjs            # glyphs where width tables disagree

XTERM_UNICODE=11 node compare.mjs claude alacritty-tail 150 | node summ.mjs
```

Candidates for `compare.mjs`: `alacritty-tail` (the winner, from `veal-core`), `vt100`,
`raw`. Leave `XTERM_UNICODE` unset for Veal's current
widths, `11` for the Unicode 11 addon, `15` for the graphemes addon.

- `g5.mjs`: a snapshot landing on a used terminal. Needs a `scrollback` comparison first.
- `truth.mjs`: scores `shell-resize` against the text that was actually printed.
- `scan.mjs <names>`: counts sequences in recordings, to see if a known gap matters.
- `inspect-cut.mjs <name> <event> <offset>`: one differing cut from `compare.mjs`, with both screens and the bytes before it.
- `eye.mjs <fraction> <name>`: a page with two real xterms, to screenshot and diff.
- `../rs/target/release/gate.exe bench ../out/flood.bin 1`: the cost check.

## The `claude` scenarios

These start a real Claude Code session and send prompts, so they use some of your
Claude usage. They also accept the trust prompt for `work/`. `claude` and
`claude-resize` send one short prompt; `claude-long` sends three, using read-only
tools on a copy of `js/` in `work/sample`.
