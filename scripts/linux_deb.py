#!/usr/bin/env python3
"""Derive and verify Debian runtime dependencies from the actual ELF, not ldd output."""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

MAX_DEB_BYTES = 30_000_000
REQUIRED = ['wl-clipboard', 'fonts-noto-cjk', 'libayatana-appindicator3-1']
ROOT = Path(__file__).resolve().parents[1]

def run(*args, cwd=None):
    return subprocess.check_output(args, cwd=cwd, text=True).strip()

def tools():
    for name in ['dpkg-shlibdeps', 'dpkg-deb', 'dpkg', 'readelf']:
        if not shutil.which(name):
            raise RuntimeError(f'Required packaging tool is missing: {name}')

def dependencies(binary):
    binary = Path(binary).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix='asa-shlibs-') as directory:
        work = Path(directory)
        (work/'debian').mkdir()
        (work/'debian/control').write_text('Source: ai-selection-assistant\nSection: utils\nPriority: optional\nMaintainer: github\n\nPackage: ai-selection-assistant\nArchitecture: any\nDescription: Runtime dependency verification\n')
        result = run('dpkg-shlibdeps', '-O', '-e'+str(binary), cwd=work)
    lines = [line.removeprefix('shlibs:Depends=') for line in result.splitlines() if line.startswith('shlibs:Depends=')]
    if len(lines) != 1 or not lines[0]:
        raise RuntimeError('No runtime dependency metadata was produced')
    deps = [entry.strip() for entry in lines[0].split(',')]
    if not any(d.startswith('libc6 (>= ') for d in deps):
        raise RuntimeError('Versioned libc6 requirement is missing')
    return deps

def parsed(dependency):
    match = re.fullmatch(r'([a-z0-9][a-z0-9+.-]*)(?::[a-z0-9-]+)?(?:\s*\((>=|=|>>|<=|<<)\s*([^() ]+)\))?', dependency)
    if not match:
        raise RuntimeError(f'Unsupported dependency expression (fail closed): {dependency}')
    return match.group(1), match.group(2), match.group(3)

def covers(actual, expected):
    name, op, minimum = parsed(expected)
    for entry in actual:
        package, supplied_op, supplied_version = parsed(entry)
        if name != package:
            continue
        if op is None:
            return True
        if op != '>=':
            raise RuntimeError(f'Unexpected generated constraint: {expected}')
        if supplied_op in ('>=', '=', '>>') and subprocess.run(['dpkg','--compare-versions',supplied_version,'ge',minimum]).returncode == 0:
            return True
    return False

def verify(package):
    package = Path(package).resolve(strict=True)
    size = package.stat().st_size
    if size >= MAX_DEB_BYTES:
        raise RuntimeError(f'DEB size budget exceeded: {size} bytes; must be smaller than {MAX_DEB_BYTES}')
    with tempfile.TemporaryDirectory(prefix='asa-deb-') as directory:
        work=Path(directory)
        subprocess.run(['dpkg-deb','-x',str(package),str(work)],check=True)
        binary=work/'usr/bin/ai-selection-assistant'
        expected=dependencies(binary)+REQUIRED
        actual=[x.strip() for x in run('dpkg-deb','-f',str(package),'Depends').split(',')]
        missing=[entry for entry in expected if not covers(actual,entry)]
        if missing:
            raise RuntimeError('Missing or insufficient DEB dependencies: '+', '.join(missing))
        architecture=run('dpkg-deb','-f',str(package),'Architecture')
        host_arch=run('dpkg','--print-architecture')
        if architecture != host_arch:
            raise RuntimeError(f'Native verification architecture mismatch: {architecture} vs {host_arch}')
        for relative in ['usr/share/applications/AI Selection Assistant.desktop','usr/share/icons/hicolor/32x32/apps/ai-selection-assistant.png','usr/share/icons/hicolor/128x128/apps/ai-selection-assistant.png']:
            if not (work/relative).is_file():
                raise RuntimeError('Missing desktop integration asset: '+relative)
        symbols=run('readelf','--wide','--dyn-syms',str(binary))
        versions=sorted(set(re.findall(r'GLIBC_(\d+\.\d+(?:\.\d+)?)',symbols)),key=lambda v:tuple(map(int,v.split('.'))))
        if not versions or not covers(actual,'libc6 (>= '+versions[-1]+')'):
            raise RuntimeError('libc6 constraint does not cover final ELF GLIBC symbol floor')
        print(json.dumps({'package':str(package),'architecture':architecture,'bytes':size,'maxBytesExclusive':MAX_DEB_BYTES,'glibcSymbolFloor':versions[-1],'depends':actual,'verified':True},ensure_ascii=False,indent=2))

def main():
    tools()
    if len(sys.argv) != 3 or sys.argv[1] not in ['config','verify']:
        raise RuntimeError('Usage: linux_deb.py config BINARY | verify PACKAGE')
    if sys.argv[1]=='config':
        deps=dependencies(sys.argv[2])+REQUIRED
        print(json.dumps({'bundle':{'linux':{'deb':{'depends':list(dict.fromkeys(deps))}}}}))
    else:
        verify(sys.argv[2])

if __name__=='__main__':
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(str(error),file=sys.stderr)
        sys.exit(1)
