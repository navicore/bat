# navicore's bat fork — renders markdown tables

This is a **personal fork** of [sharkdp/bat](https://github.com/sharkdp/bat).
It is **not maintained as an upstream contribution and is not ready for a PR**
as-is. It is public in case someone else wants to carry the idea upstream —
see *"If you want to make this PR-worthy"* below.

## What this fork adds

`bat some-file.md` now **renders GFM pipe tables**: cell padding is inserted
so columns line up, and the table structure is colored.

- Tables are aligned only when writing to a terminal (or to a pager that bat
  spawns itself). **Piped or redirected output is byte-for-byte identical to
  the input** — `bat file.md > out.md` never modifies your file.
- Alignment is whitespace-only and preserves the exact line count, so line
  numbers, `--line-range`, git-diff markers and decorations are unaffected.
- GFM rules are respected: tables cannot interrupt a paragraph (a pipe-line
  directly after paragraph text is left untouched), fenced code blocks are
  skipped, `\|` escapes are honored, and `:--:` alignment colons survive.
- The default themes color the table: **Monokai Extended** / **Monokai
  Extended Light** carry injected rules (pipes & separator row in the theme's
  comment gray, header row in the theme's heading hue). **Nord** already had
  pipe rules upstream.

Everything else about bat is unchanged.

## Install (notes to myself)

```sh
cd ~/git/navicore/bat
git switch navicore-tables
cargo install --path . --force --root ~/.local   # installs to ~/.local/bin/bat
```

Careful: this machine also has Debian's `/usr/bin/batcat` symlinked as
`~/.local/bin/bat` (v0.24.0, without this feature). After installing, check
`type -a bat` — the first hit wins. `bat --version` will *not* distinguish
the fork (same version number); test with any markdown file containing a
table, or:

```sh
printf '| a | bbb |\n|---|---|\n| c | d |\n' | bat -l md
# fork: columns aligned. upstream: ragged.
```

## Known limitations

- Tables wider than the terminal wrap (obviously). Combine with
  `--wrap=never` on wide panes.
- Tables containing tab characters are skipped (tab width is
  terminal-dependent).
- An interactively-copied table is padded but semantically identical
  markdown.

## How it works (for anyone wanting to take this upstream)

| File | Change |
|---|---|
| `src/markdown_tables.rs` | New. GFM table detection (fence-aware, block-boundary-aware) + column alignment with `unicode-width`. 16 unit tests. |
| `src/input.rs` | `OpenedInput::realign_markdown_tables()` — drains the reader, aligns, swaps in a fresh reader over the aligned bytes. No-op for non-UTF-8/UTF-16/binary. |
| `src/controller.rs` | Gate: align only when stdout is a terminal, the resolved syntax is `Markdown`, and not `--loop-through`. |
| `assets/patches/Monokai-Extended.tmTheme.patch` | Canonical theme rules (dark theme), consumed by `assets/create.sh` when regenerating assets. |
| `assets/themes.bin` | The committed binary asset was updated **in-place** to match (dark + Light themes; the Light rules exist only here — there is no patch file for Light). |

Relevant upstream context: the syntax already tokenizes tables (scopes
`meta.table.*`, `punctuation.separator.table-cell.markdown`); the gap is that
no bundled theme colors them ([sharkdp/bat#1752](https://github.com/sharkdp/bat/issues/1752))
and nothing aligns geometry. Alignment-before-highlighting is the trick: pad
the raw text, then let the normal pipeline highlight it — no style surgery.

Things upstream would likely want changed before a PR:

- A CLI/config opt-in or opt-out (this fork aligns unconditionally for
  interactive markdown).
- Streaming-friendly implementation (this fork buffers the whole input for
  markdown files; upstream may want the aligner integrated into the
  controller's line loop instead).
- Scope-based table detection via syntect rather than textual heuristics.
- Theme changes as separate PRs, and regenerated via `create.sh` with
  submodules initialized rather than editing `themes.bin` directly.

Test with `cargo test` (471 tests, incl. 16 for the table module).
