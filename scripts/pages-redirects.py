#!/usr/bin/env python3
"""Write the GitHub Pages site: a redirect stub for every rendered docs page.

The documentation lives at https://phit.phpstats.org/docs, which imports the
HTML committed under docs/. GitHub Pages keeps serving the old addresses, so
docs/x.html becomes a stub that redirects to /docs/x, and docs/index.html to
/docs. Usage: python3 scripts/pages-redirects.py OUTPUT_DIR
"""
import html
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / 'docs'
BASE = 'https://phit.phpstats.org/docs'


def stub(url):
    href = html.escape(url, quote=True)
    return f'''<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>phit documentation has moved</title>
<link rel="canonical" href="{href}">
<meta name="robots" content="noindex">
<meta http-equiv="refresh" content="0; url={href}">
<script>location.replace("{href}" + location.hash);</script>
</head>
<body>
<p>The phit documentation is now at <a href="{href}">{href}</a>.</p>
</body>
</html>
'''


def main():
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    count = 0
    for page in sorted(DOCS.rglob('*.html')):
        relative = page.relative_to(DOCS)
        if relative.parts[0].startswith('_') or relative.parts[0] == 'site_libs':
            continue
        path = relative.with_suffix('').as_posix()
        url = BASE if path == 'index' else f'{BASE}/{path}'
        (out / relative).parent.mkdir(parents=True, exist_ok=True)
        (out / relative).write_text(stub(url))
        count += 1
    (out / '404.html').write_text(stub(BASE))
    (out / '.nojekyll').write_text('')
    print(f'Wrote {count} redirect pages to {out}')


if __name__ == '__main__':
    main()
