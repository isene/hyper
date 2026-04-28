# Hyper — manual smoke-test plan (v0.4.x)

Run each session in order. Total time ~5 minutes. Use a **throwaway copy** of any
file you care about before the editing/encryption sessions.

## Prerequisites

```bash
# Build
cd /home/geir/Main/G/GIT-isene/hyper
PATH="/usr/bin:$PATH" cargo build --release
# ~/bin/hyper symlinks to target/release/hyper — already up-to-date.

# Pick a non-encrypted .hl file for read-time tests
SAMPLE=/home/geir/G/Hl/Personal.hl    # adjust to taste

# Throwaway scratch file for editing tests
cp "$SAMPLE" /tmp/test.hl
```

---

## Session 1 — Phase 1 (read-time helpers)

```bash
hyper "$SAMPLE"
```

| Press | Expected |
|---|---|
| `3` | Fold to level 3 |
| `0` | Collapse everything (only top-level items visible) |
| `Z` | Expand all |
| `S` then keyword + Enter | Show only items containing the word (+ ancestors) |
| `F` | Clear filter |
| `H` then keyword + Enter | Hide items containing the word |
| `F` | Clear filter |
| `*` | Branch dim ON; `*` again → OFF |
| `p` | Presentation mode ON; `p` again → OFF |
| `t` | Jump to next item ending in `=` |
| Move to an item with `<ref>`, press `ENTER` | Jumps to the referenced item |
| Move to an item with `<file:~/foo>`, press `ENTER` | `xdg-open` fires |
| `C` | Complexity popup (items × (1 + max_depth/10)); any key closes |
| `?` | Help; any key closes |
| `Q` | Quit, no save |

---

## Session 2 — Phase 2 (edit mode)

```bash
hyper /tmp/test.hl
```

| Press | Expected |
|---|---|
| `i` | Footer prompt with current item text; edit + Enter |
| `+` | New sibling below; type text + Enter (empty input cancels) |
| `O` | New sibling above |
| `Tab` | Indent current subtree by one level |
| `Shift-Tab` | Outdent (no-op at depth 0) |
| `v` | Checkbox cycle: empty → `[_]` → `[x]` → empty |
| `V` | Same cycle, `[x]` includes today's `(YYYY-MM-DD)` stamp |
| `R` | Renumber whole document — every level gets `1.`, `2.`, … |
| `D` | Delete current item AND its subtree |
| `W` | Save; footer shows green " Saved" |
| `q` | Quit (saves on exit if dirty) |

After quitting, inspect the file:

```bash
cat /tmp/test.hl
```

---

## Session 3 — Phase 3 (exporters + calendar + completion)

```bash
hyper /tmp/test.hl
```

| Press | Expected |
|---|---|
| `M-h` | Writes `/tmp/test.html` next to source |
| `M-l` | Writes `/tmp/test.tex` |
| `M-m` | Writes `/tmp/test.md` |
| `M-g` | One `.ics` per future-dated item in `~/.tock/incoming/` |
| `M-c` | Operator/property popup; press `1`-`9`/`0` to insert; any other key cancels |

Verify exports:

```bash
ls -la /tmp/test.html /tmp/test.tex /tmp/test.md
ls ~/.tock/incoming/ | grep hyper_
firefox /tmp/test.html      # visual check of HTML
xelatex /tmp/test.tex       # if you have a LaTeX install
```

---

## Session 4 — gpg encryption against `.p.hl`

> **Use a copy first.** Once round-trip is confirmed, rerun against `~/.p.hl`
> directly if you want.

```bash
cp ~/.p.hl /tmp/p-test.hl
hyper /tmp/p-test.hl
```

| Press | Expected |
|---|---|
| `X` | Passphrase prompt on `/dev/tty` (echo off). Doc loads in plaintext |
| (read content; should match what hyperlist.vim shows for `.p.hl`) | |
| `E` | Passphrase prompt; file rewritten armored. In-memory doc shows `⊟ENC-FILE: <path>` marker |
| `q` | Quit |

Verify round-trip:

```bash
diff <(gpg -d ~/.p.hl 2>/dev/null) <(gpg -d /tmp/p-test.hl 2>/dev/null)
# Empty output = round-trip identical plaintext.
```

### Subtree encryption (without touching `.p.hl`)

```bash
hyper /tmp/test.hl
```

| Press | Expected |
|---|---|
| Move to an item, press `e` | Subtree replaced by `⊟ENC: <armored>` sentinel |
| `x` on the sentinel | Plaintext spliced back in |
| `W` | Save |

---

## Known notes / gotchas

- Passphrase prompt opens `/dev/tty` directly; works inside `tmux`.
- `gpg-agent` caches the passphrase if configured — subsequent calls in the
  same session won't re-prompt.
- `Tock` picks up `.ics` files from `~/.tock/incoming/`. Calendar export uses
  the same hand-off pipe as kastrup's `Z` action.
- `M-h` / `M-l` / `M-m` / `M-g` / `M-c` rely on Alt-key delivery from your
  terminal. If they don't fire, run `kitty +kitten show_key` (or equivalent
  for your term) on Alt-h and report what byte sequence comes through — we'll
  add the alternative form.
- `a`–`f` are intentionally unbound; reserved for future hyperlist.vim parity
  bindings.
- `0` folds to level 1 (root only); `Z` expands all.

---

## Quick regression sweep (after any change)

```bash
cd /home/geir/Main/G/GIT-isene/hyper
PATH="/usr/bin:$PATH" cargo build --release && \
  echo "1. open" && hyper "$SAMPLE" </dev/null  # eyeball that it renders
# (interactive inspection of new behaviour)
```
