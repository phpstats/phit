#!/usr/bin/env python3
"""Package the already tested Linux NTS artifact, ABI facts and dependency notices.

The archive is named as PIE's pre-packaged-binary download method expects:
php_{extension}-{tag}_php{X.Y}-{arch}-linux-glibc-nts.zip, with phit.so at the
archive root.
"""
from pathlib import Path
import hashlib, json, os, re, shutil, subprocess, sys, tempfile, tomllib, zipfile

ROOT = Path(__file__).resolve().parents[1]

def output(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

php = os.environ.get('PHP', 'php')
php_config = os.environ.get('PHP_CONFIG', 'php-config')
platform = json.loads(output(php, '-r', '''echo json_encode([
    'php'=>PHP_VERSION, 'os'=>PHP_OS_FAMILY, 'architecture'=>php_uname('m'),
    'zts'=>(bool)PHP_ZTS, 'debug'=>(bool)PHP_DEBUG, 'version'=>phpversion('phit')]);'''))
if platform['os'] != 'Linux' or platform['zts'] or platform['debug']:
    raise SystemExit('Binary packaging supports Linux NTS non-debug builds only')
version = tomllib.loads((ROOT/'crates/php-extension/Cargo.toml').read_text())['package']['version']
if platform['version'] != version:
    raise SystemExit('Loaded extension and source crate versions differ')
artifact = Path(output(php_config, '--extension-dir'))/'phit.so'
if not artifact.is_file():
    raise SystemExit('Install and verify the extension before packaging')
target = next(line.removeprefix('host: ') for line in output('rustc', '-vV').splitlines() if line.startswith('host: '))
metadata = json.loads(output('cargo', 'metadata', '--locked', '--format-version', '1', '--filter-platform', target))
packages = {p['id']: p for p in metadata['packages']}
nodes = {n['id']: n for n in metadata['resolve']['nodes']}
seen = set()

def visit(key):
    if key in seen:
        return
    seen.add(key)
    for dep in nodes[key]['deps']:
        if any(kind['kind'] != 'dev' for kind in dep['dep_kinds']):
            visit(dep['pkg'])

visit(next(p['id'] for p in packages.values() if p['name'] == 'phit'))
notices = ['Dependency notices (including build dependencies). Project-owned code: MIT.\n']
provenance = []
for key in sorted(seen, key=lambda k: (packages[k]['name'], packages[k]['version'])):
    package = packages[key]
    if package['source'] is None:
        continue
    directory = Path(package['manifest_path']).parent
    files = sorted(f for f in directory.iterdir() if f.is_file() and f.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE')))
    if package.get('license_file'):
        license_file = directory/package['license_file']
        if license_file not in files:
            files.append(license_file)
    if not files:
        files = [ROOT/'third-party'/f"{package['name']}-{package['version']}"/'LICENSE-MIT']
    if any(not f.is_file() for f in files):
        raise SystemExit(f"Missing dependency notice: {package['name']} {package['version']}")
    record = dict(name=package['name'], version=package['version'], license=package['license'], repository=package['repository'], notices=[])
    notices.append(f"\n{'='*72}\n{package['name']} {package['version']} — {package['license']}\n{package['repository']}\n")
    for file in files:
        text = file.read_text()
        notices.append(f'\n--- {file.name} ---\n{text}\n')
        record['notices'].append(dict(file=file.name, sha256=sha(file)))
    provenance.append(record)

# The distribution explicitly targets the pinned Debian Bookworm environment;
# this artifact is not advertised as portable to Alpine/musl or older glibc.
php_minor = '.'.join(platform['php'].split('.')[:2])
# PIE names architectures x86_64 and arm64.
arch = {'aarch64': 'arm64', 'amd64': 'x86_64'}.get(platform['architecture'], platform['architecture'])
name = f"php_phit-v{version}_php{php_minor}-{arch}-linux-glibc-nts"
symbols = output('objdump', '-T', str(artifact))
glibc = max((tuple(map(int, v.split('.'))) for v in re.findall(r'GLIBC_([0-9.]+)', symbols)), default=())
out = Path(sys.argv[1] if len(sys.argv) > 1 else ROOT/'dist')
out.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory() as temporary:
    payload = Path(temporary)/name
    payload.mkdir()
    shutil.copyfile(artifact, payload/'phit.so')
    shutil.copyfile(ROOT/'LICENSE', payload/'LICENSE')
    (payload/'THIRD_PARTY_NOTICES.txt').write_text(''.join(notices))
    report = dict(schema_version=1, artifact=name, **platform, rust=output('rustc', '--version'),
                  target=target, php_api=re.search(r'^#define\s+PHP_API_VERSION\s+(\d+)',
                      (Path(output(php_config, '--include-dir'))/'main/php.h').read_text(), re.MULTILINE).group(1),
                  libc=output('ldd', '--version').splitlines()[0],
                  minimum_glibc='.'.join(map(str, glibc)),
                  dynamic_dependencies=output('ldd', str(artifact)),
                  cargo_lock_sha256=sha(ROOT/'Cargo.lock'), extension_sha256=sha(artifact), dependencies=provenance)
    (payload/'manifest.json').write_text(json.dumps(report, indent=2)+'\n')
    (payload/'INSTALL.txt').write_text(
        f'Built and tested for PHP {php_minor}, {arch}, NTS, non-debug, on Debian Bookworm.\n'
        f'Needs glibc {report["minimum_glibc"]} or later. manifest.json lists the full ABI.\n'
        'pie install phpstats/phit picks this archive by its name.\n'
        'To install by hand, copy phit.so to php-config --extension-dir.\n'
        'Add extension=phit.so to the CLI and FPM configuration; restart FPM.\n'
        'Verify extension_loaded("phit") and native model calls in both runtimes.\n'
        'Keep LICENSE and THIRD_PARTY_NOTICES.txt when redistributing.\n')
    archive = out/f'{name}.zip'
    with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as zip_file:
        for path in sorted(payload.iterdir()):
            info = zipfile.ZipInfo(path.name, date_time=(1980, 1, 1, 0, 0, 0))
            info.external_attr = 0o644 << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            zip_file.writestr(info, path.read_bytes())
    (out/f'{name}.sha256').write_text(f'{sha(archive)}  {archive.name}\n')
    print(archive)
