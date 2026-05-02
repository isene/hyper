# Hyper — archived

> **This project is archived.** Its full feature set has been folded into [**scribe**](https://github.com/isene/scribe) as of v0.1.28. Use scribe instead.

## Why archived

Scribe is a writer's editor in the [Fe₂O₃ Rust terminal suite](https://github.com/isene/fe2o3) with vim-flavoured modal editing, Claude Code in the editor, registers, marks, macros, spellcheck, reading mode, and more. It now carries full `hyperlist.vim` parity, so a single editor handles prose AND HyperList editing — and the HyperList work in scribe gets all the other niceties (AI integration, persistent registers, sessions) for free.

Hyper, on the other hand, is a HyperList-only TUI that lacks all of those. Maintaining two HL editors isn't worth it.

## What scribe gives you that hyper used to

- **Folding** by indent: `<SPACE>` toggle, `<C-SPACE>` recursive, `\0`..`\9` `\a`..`\f` set fold level
- **References**: `gr` jumps, `gf` opens external files
- **Checkboxes**: `\v` / `\V` (timestamp) / `\o` (in-progress)
- **Autonumbering**: `\an` toggle, `Ctrl-T` / `Ctrl-D` indent + renumber, `\R` renumber selection
- **Sort by indent**: `\s` (visual)
- **Presentation traversal**: `g<DOWN>` / `g<UP>`
- **Show / hide word**: `zs` / `zh` / `z0`
- **Encryption**: `\z` / `\Z` / `\x` / `\X` — byte-for-byte compatible with the Ruby `hyperlist` app's `ENC:` format. Auto-decrypt on open for `.foo.hl` dotfiles or any file with `ENC:` header. Your existing password files (`/home/.safe/.p2.hl` etc.) work unchanged.
- **Exports**: `\H` HTML, `\L` LaTeX, `\M` Markdown, or `:export FMT`
- **Calendar add**: `\G` posts items with future dates to gcalcli or writes `.ics`
- **Complexity stat**: `\C`
- **Limelight-style highlight**: `\h`
- In-editor docs: `:h hl` or `:h hyperlist`

## Migration

```bash
# build / update scribe
cd ~/Main/G/GIT-isene/scribe
git pull
PATH="/usr/bin:$PATH" cargo build --release

# repoint anything that opened hyper
ln -sf "$PWD/target/release/scribe" ~/bin/hyper   # opt — keeps muscle memory
# or update aliases / utilities-menu entries
```

## License

Public domain ([Unlicense](https://unlicense.org/)).
