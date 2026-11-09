"""Re-elaborate the designated proofs and reject unexpected transitive axioms."""
import pathlib
import re
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parents[1]
lake = str(pathlib.Path(sys.argv[1]).resolve())
run = subprocess.run(
    [lake, 'env', 'lean', 'Frith/Proofs.lean'], cwd=root / 'formal',
    text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
)
print(run.stdout, end='')
if run.returncode or re.search(r'\b(warning|error):', run.stdout):
    raise SystemExit('Proof elaboration failed or emitted warnings')
expected = {
    'checkExport_sound', 'checkExport_complete', 'reachable_invariant',
    'revoked_terminal', 'stale_revision_denied', 'manifest_preserved',
    'revision_increases', 'authorization_requires_policy', 'publication_requires_policy',
}
allowed = {'propext', 'Classical.choice', 'Quot.sound'}
seen = set()
for name, axioms in re.findall(r"'Frith\.(\w+)' depends on axioms: \[([^\]]*)\]", run.stdout):
    if name in seen or name not in expected:
        raise SystemExit(f'Unexpected or duplicate theorem audit: {name}')
    seen.add(name)
    used = {a.strip() for a in axioms.split(',') if a.strip()}
    if not used <= allowed:
        raise SystemExit(f'Unexpected axioms in {name}: {used - allowed}')
if seen != expected:
    raise SystemExit(f'Missing theorem audits: {expected - seen}')
print(f'Audited {len(seen)} release theorems: only standard Lean axioms.')
