#!/usr/bin/env python3
"""Extract and reinsert Rust comments, for the move to English.

The translation itself is done by hand (see docs/TRANSLATION.md); this only
moves the text safely in and out of the source, which is the part that must
not go wrong across 241 files.

    tools/translate_comments.py extract <path...>  > batch.json
    tools/translate_comments.py verify  batch.json      # round-trip, no writes
    tools/translate_comments.py apply   batch.json      # writes the files

A comment is only a comment when the `//` is not inside a string. `"http://x"`
and `r#"a // b"#` are not comments, and a tool that thinks they are will eat
code. `comment_start` is the whole safety story, so `verify` re-extracts after
a no-op apply and demands the bytes come back identical.
"""
import json
import pathlib
import sys


def comment_start(line: str) -> int:
    """Index of the `//` that starts a real comment, or -1.

    Walks the line tracking normal strings, raw strings (`r"`, `r#"`..) and
    escapes, so a `//` inside any of them is not mistaken for a comment.
    """
    i, n = 0, len(line)
    state = None   # None | '"' | 'raw'
    hashes = 0
    while i < n:
        c = line[i]
        if state is None:
            if c == 'r' and i + 1 < n and line[i + 1] in '"#':
                j, h = i + 1, 0
                while j < n and line[j] == '#':
                    h += 1
                    j += 1
                if j < n and line[j] == '"':
                    state, hashes, i = 'raw', h, j + 1
                    continue
            if c == "'" and i + 2 < n and (line[i + 1] != '\\' and line[i + 2] == "'"):
                i += 3          # char literal
                continue
            if c == '"':
                state, i = '"', i + 1
                continue
            if c == '/' and i + 1 < n and line[i + 1] == '/':
                return i
            i += 1
        elif state == '"':
            if c == '\\':
                i += 2
                continue
            if c == '"':
                state = None
            i += 1
        else:                    # raw string
            if c == '"':
                j, h = i + 1, 0
                while j < n and line[j] == '#' and h < hashes:
                    h += 1
                    j += 1
                if h == hashes:
                    state, i = None, j
                    continue
            i += 1
    return -1


def split(line: str):
    """(before, marker, text) for a comment line, or None.

    `marker` keeps the flavour — `//`, `///`, `//!` — because doc comments are
    part of the API and turning one into the other changes what rustdoc emits.
    """
    i = comment_start(line)
    if i < 0:
        return None
    rest = line[i:]
    marker = '//'
    for m in ('///', '//!'):
        if rest.startswith(m):
            marker = m
            break
    return line[:i], marker, rest[len(marker):]


def rust_files(paths):
    for arg in paths:
        p = pathlib.Path(arg)
        if p.is_dir():
            yield from (q for q in sorted(p.rglob('*.rs')) if 'target' not in q.parts)
        elif p.suffix == '.rs':
            yield p


def cmd_extract(paths):
    """Group consecutive whole-line comments into BLOCKS.

    English does not wrap where Portuguese wraps, so a line-for-line mapping
    would force the translation to keep someone else's line breaks. A block
    is translated as a unit and re-emitted at whatever length it needs.

    A trailing comment (`code // note`) is its own block of one, because its
    line also holds code and cannot grow.
    """
    out = []
    for p in rust_files(paths):
        lines = p.read_text(encoding='utf-8').splitlines()
        cur = None
        for n, line in enumerate(lines):
            s = split(line)
            whole = bool(s) and s[0].strip() == ''
            if not s or not s[2].strip():
                cur = None
                continue
            if (cur and whole and cur['whole']
                    and cur['before'] == s[0] and cur['marker'] == s[1]
                    and cur['line'] + len(cur['pt']) == n):
                cur['pt'].append(s[2])
                continue
            cur = {'file': str(p), 'line': n, 'before': s[0], 'marker': s[1],
                   'whole': whole, 'pt': [s[2]], 'en': None}
            out.append(cur)
            if not whole:
                cur = None
    json.dump(out, sys.stdout, ensure_ascii=False, indent=1)


def write_back(items, dry):
    """Rebuild each touched file from its blocks. Returns {file: changed}.

    Blocks are replaced back to front so earlier line numbers stay valid
    while later ones are still being spliced.
    """
    by_file = {}
    for it in items:
        by_file.setdefault(it['file'], []).append(it)
    changed = {}
    for f, its in by_file.items():
        p = pathlib.Path(f)
        original = p.read_text(encoding='utf-8')
        lines = original.splitlines()
        for it in sorted(its, key=lambda x: x['line'], reverse=True):
            n, pt = it['line'], it['pt']
            # The block must still be the one we extracted, or the file moved
            # under us and the line numbers are lies.
            for k, want in enumerate(pt):
                got = lines[n + k]
                assert got == it['before'] + it['marker'] + want, \
                    f"{f}:{n + k + 1} moved since extract"
            text = it['en'] if it.get('en') is not None else pt
            lines[n:n + len(pt)] = [it['before'] + it['marker'] + t for t in text]
        new = '\n'.join(lines) + ('\n' if original.endswith('\n') else '')
        changed[f] = new != original
        if not dry:
            p.write_text(new, encoding='utf-8')
    return changed


def cmd_verify(batch):
    """A no-op apply must leave every byte alone."""
    items = json.loads(pathlib.Path(batch).read_text())
    for it in items:
        it['en'] = None
    changed = write_back(items, dry=True)
    bad = [f for f, c in changed.items() if c]
    if bad:
        print('ROUND-TRIP CHANGED BYTES:', *bad, sep='\n  ')
        return 1
    n = sum(len(i['pt']) for i in items)
    print(f'round-trip clean: {n} lines in {len(items)} blocks '
          f'across {len(changed)} files')
    return 0


def cmd_apply(batch):
    items = json.loads(pathlib.Path(batch).read_text())
    todo = [it for it in items if it.get('en') is not None]
    changed = write_back(items, dry=False)
    print(f'{len(todo)} comments written across {sum(changed.values())} files')
    return 0


if __name__ == '__main__':
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    cmd, rest = sys.argv[1], sys.argv[2:]
    sys.exit({'extract': lambda: cmd_extract(rest),
              'verify': lambda: cmd_verify(rest[0]),
              'apply': lambda: cmd_apply(rest[0])}[cmd]() or 0)
