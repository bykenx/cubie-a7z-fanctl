#!/usr/bin/env python3
"""Generate deduplicated notices for the target's non-dev Cargo dependency graph."""
import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', help='Rust target triple (default: rustc host)')
    parser.add_argument('--output', type=Path, default=Path('target/THIRD-PARTY-LICENSES.txt'))
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    cargo = shlex.split(os.environ.get('CARGO', 'cargo'))
    target = args.target or os.environ.get('CARGO_BUILD_TARGET')
    if not target:
        # Use Cargo's selected toolchain, including `cargo +VERSION` overrides.
        rustc = ['rustc'] + [arg for arg in cargo[1:] if arg.startswith('+')]
        version = subprocess.check_output(rustc + ['-vV'], text=True)
        target = next(line.removeprefix('host: ') for line in version.splitlines()
                      if line.startswith('host: '))
    metadata = json.loads(subprocess.check_output(
        cargo + ['metadata', '--locked', '--offline', '--format-version=1',
                 '--filter-platform', target], cwd=root))
    nodes = {node['id']: node for node in metadata['resolve']['nodes']}
    selected = set()
    pending = [metadata['resolve']['root']]
    while pending:
        package_id = pending.pop()
        if package_id in selected:
            continue
        selected.add(package_id)
        for dep in nodes[package_id]['deps']:
            if any(kind['kind'] != 'dev' for kind in dep['dep_kinds']):
                pending.append(dep['pkg'])

    # Store each exact license/notice text once; keep the attribution in full.
    texts = {}
    packages = []
    for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        if package['id'] not in selected or package['source'] is None:
            continue
        crate = Path(package['manifest_path']).parent
        files = sorted(p for p in crate.iterdir() if p.is_file() and
                       p.name.upper().startswith(('LICENSE', 'COPYING', 'COPYRIGHT', 'NOTICE')))
        if not files:
            raise SystemExit(f'No license texts found: {package["name"]}')
        refs = []
        for path in files:
            text = path.read_text().replace('\r\n', '\n').strip()
            if not text:
                continue
            if text not in texts:
                texts[text] = f'L{len(texts) + 1}'
            refs.append(f'{path.name}={texts[text]}')
        if not refs:
            raise SystemExit(f'Empty license texts: {package["name"]}')
        packages.append(f'{package["name"]} {package["version"]}\n'
                        f'License: {package.get("license") or "see texts"}\n'
                        + ', '.join(refs))
    sections = [f'Third-party dependency notices\nTarget: {target}\n'
                'Includes normal/build dependencies; excludes dev-only dependencies.\n'
                'Identical license texts are shared; original attributions are retained.',
                '\n\n'.join(packages)]
    sections += [f'[{ref}]\n{text}' for text, ref in texts.items()]
    output = args.output if args.output.is_absolute() else root / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text('\n\n'.join(sections) + '\n')
    print(f'{output}: {len(packages)} dependencies, {len(texts)} unique texts')


if __name__ == '__main__':
    main()
