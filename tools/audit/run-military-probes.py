#!/usr/bin/env python3
"""Run diagnostic probes in a disposable copy of a pinned Git snapshot.

PASS means the documented behaviour was reproduced, NOT that it is correct.
The active checkout and production logic are never modified. Requires cargo.
"""
import argparse
import io
from pathlib import Path
import subprocess
import tarfile
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--ref', default='HEAD', help='Git snapshot to audit')
parser.add_argument('--cargo', default='cargo', help='Cargo executable or wrapper')
args = parser.parse_args()
repo = Path(__file__).resolve().parents[2]
sha = subprocess.check_output(['git', 'rev-parse', '--verify', args.ref+'^{commit}'], cwd=repo, text=True).strip()
source = Path(__file__).with_name('military_probes.rs').read_text()
archive = subprocess.check_output(['git', 'archive', sha], cwd=repo)
with tempfile.TemporaryDirectory(prefix='hoi4-audit-') as directory:
    work = Path(directory)
    with tarfile.open(fileobj=io.BytesIO(archive)) as bundle:
        bundle.extractall(work, filter='data')
    (work / 'Cargo.lock').write_bytes(Path(__file__).with_name('cargo-ac0ee26.lock').read_bytes())
    target = work / 'src' / 'support_tests.rs'
    with target.open('a') as out:
        out.write('\n'+source)
    print('Snapshot:', sha, flush=True)
    print('Diagnostic probes: PASS reproduces the documented issue.', flush=True)
    result = subprocess.run([args.cargo, 'test', '--locked', '--bin', 'hoi4-mobile', '-j1', 'support_tests::audit_probes', '--', '--nocapture', '--test-threads=1'], cwd=work)
    raise SystemExit(result.returncode)
