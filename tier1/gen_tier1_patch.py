"""Generate tier1.patch mechanically from exact string replacements.

Regenerates tier1/tier1.patch from a pristine idna 3.20 core.py, so the
patch never drifts from its anchors by hand-editing.
Usage: python tier1/gen_tier1_patch.py <stock-core.py> <patched-out.py>
Then: diff -U3 --label=a/core.py --label=b/core.py <stock> <patched>.
"""
import subprocess
import sys

V = sys.argv[1]
OUT = sys.argv[2]
with open(V) as f:
    src = f.read()

def rep(old, new):
    global src
    assert src.count(old) == 1, f"anchor not unique/found: {old[:70]!r} count={src.count(old)}"
    src = src.replace(old, new)

rep("from . import idnadata\nfrom .intranges import intranges_contain\n",
    "from . import idnadata\nfrom .intranges import intranges_contain\n"
    "from .uts46data import uts46_replacements, uts46_starts, uts46_statuses\n")

rep('_std3_disallowed_re = re.compile("[\\x00-\\x2c\\x2f\\x3a-\\x40A-Z\\x5b-\\x60\\x7b-\\x7f]")\n_bidi_rtl_first',
    '_std3_disallowed_re = re.compile("[\\x00-\\x2c\\x2f\\x3a-\\x40A-Z\\x5b-\\x60\\x7b-\\x7f]")\n'
    '# Lowercase ASCII labels that are valid without further checks: 1-63 chars,\n'
    '# alnum ends, no "--" in positions 3-4 (which check_hyphen_ok rejects, and\n'
    '# which also keeps every "xn--" A-label on the slow path). Such labels are\n'
    '# NFC-stable, never start with a combiner, are all-PVALID, and contain no\n'
    '# RTL characters, so check_label/check_bidi/check_nfc are all no-ops.\n'
    '_ascii_label_ok_re = re.compile(r"(?!..--)[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\\Z")\n'
    '_bidi_rtl_first')

rep('    if len(label) == 0:\n        raise IDNAError("Empty Label", code="empty_label")\n\n    # Check against the domain length',
    '    if len(label) == 0:\n        raise IDNAError("Empty Label", code="empty_label")\n\n'
    '    if isinstance(label, str) and _ascii_label_ok_re.match(label) is not None:\n'
    '        # Fast path: the match implies every check below passes (see above).\n'
    '        return None\n\n    # Check against the domain length')

rep('    if len(label) > _max_input_length:\n        raise IDNAError("Label too long", code="input_too_long")\n    try:\n        label_bytes = label.encode("ascii")',
    '    if len(label) > _max_input_length:\n        raise IDNAError("Label too long", code="input_too_long")\n'
    '    if isinstance(label, str) and _ascii_label_ok_re.match(label) is not None:\n'
    '        # Same bytes the ulabel roundtrip below would return: the label is\n'
    '        # already lowercase ASCII, never an "xn--" A-label, and valid.\n'
    '        return label.encode("ascii")\n\n    try:\n        label_bytes = label.encode("ascii")')

rep('    if len(label) > _max_input_length:\n        raise IDNAError("Label too long", code="input_too_long")\n    if not isinstance(label, (bytes, bytearray)):',
    '    if len(label) > _max_input_length:\n        raise IDNAError("Label too long", code="input_too_long")\n'
    '    if isinstance(label, str) and _ascii_label_ok_re.match(label) is not None:\n'
    '        # The slow path below would lowercase (a no-op), skip the "xn--"\n'
    '        # branch, validate via check_label (a no-op here), and return the\n'
    '        # label unchanged.\n'
    '        return label\n\n    if not isinstance(label, (bytes, bytearray)):')

rep('        return result\n\n    from .uts46data import uts46_replacements, uts46_starts, uts46_statuses\n\n    # ``start`` marks the run',
    '        return result\n\n    # ``start`` marks the run')

with open(OUT, "w") as f:
    f.write(src)
print("replacements OK")
