#!/usr/bin/env python3
"""Checks that the committed HTML in docs/ was rendered from the current .qmd sources.

The site serves docs/*.html as committed (scripts/render-docs copies Quarto's output there), so a source edited
without a re-render would publish stale pages. For every page, each fenced code or output block in the .qmd
(after expanding {{< include >}}) must appear in the rendered HTML's code blocks, in order. Executable chunks
({python} and the like) and raw blocks ({=html}) are left out, since their rendered form differs from the source.

File modification times are not compared: a git checkout sets them all to checkout time.

Usage: python3 scripts/docs-rendered.py [--check]   (the check is the only mode)
"""
import html
import re
import sys
from pathlib import Path

DOCS = Path(__file__).resolve().parents[1] / 'docs'
FENCE = re.compile(r'^(`{3,})([^\n`]*)\n(.*?)^\1[ \t]*$', re.S | re.M)
INCLUDE = re.compile(r'\{\{<\s*include\s+(\S+)\s*>\}\}')
PRE = re.compile(r'<pre[^>]*>(.*?)</pre>', re.S)


def normalize(text):
    return '\n'.join(line.rstrip() for line in text.strip('\n').split('\n'))


def source_blocks(path):
    text = INCLUDE.sub(lambda m: (path.parent / m.group(1)).read_text(), path.read_text())
    blocks = []
    for m in FENCE.finditer(text):
        info = m.group(2).strip()
        if info.startswith('{'):
            continue
        blocks.append(normalize(m.group(3)))
    return [b for b in blocks if b]


def rendered_blocks(path):
    blocks = []
    for m in PRE.finditer(path.read_text()):
        # Quarto wraps each line in spans and links; the code is the text with the tags removed.
        blocks.append(normalize(html.unescape(re.sub(r'<[^>]+>', '', m.group(1)))))
    return blocks


def main():
    if not (DOCS / 'index.html').is_file():
        # The Docker build context and the source archive leave the rendered site out.
        print('docs-rendered: skipped (no rendered site in docs/)')
        return
    pages = sorted(p for p in DOCS.rglob('*.qmd')
                   if not any(part.startswith(('_', '.')) for part in p.relative_to(DOCS).parts))
    stale = []
    for page in pages:
        rendered = page.with_suffix('.html')
        name = page.relative_to(DOCS)
        if not rendered.is_file():
            stale.append(f'{name}: no rendered {rendered.name}')
            continue
        remaining = iter(rendered_blocks(rendered))
        for block in source_blocks(page):
            if not any(block == candidate for candidate in remaining):
                first = block.split('\n', 1)[0][:70]
                stale.append(f'{name}: a code block is missing from {rendered.name} or out of order: {first!r}')
                break
    if stale:
        print('Rendered docs are out of date; run ./scripts/render-docs and commit the HTML:', file=sys.stderr)
        for line in stale:
            print(f'  {line}', file=sys.stderr)
        sys.exit(1)
    print(f'Rendered docs match their sources ({len(pages)} pages)')


if __name__ == '__main__':
    main()
