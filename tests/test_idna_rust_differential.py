"""Kernel-vs-pure differential regression (runs against the built package).

Compares the Rust kernel (`idna._rust`) against the pure-Python path on a
fixed corpus, asserting identical return values AND identical exception
types/codes/messages. Skipped (trivially passing) when the extension is
absent -- the fallback is the same code the kernel is compared against.

Corpus: adversarial labels x flag sets + seeded fuzz domains. Frozen
2026-10-29; regenerate only with a corpus-bump commit.
"""

import random

import pytest

import idna
from idna import core

try:
    from idna import _rust  # noqa: F401

    HAVE_RUST = True
except ImportError:
    HAVE_RUST = False

NUL = chr(0)
DEL = chr(127)

LABELS = [
    "a", "Z", "0", "-", "a-", "-ab", "ab", "a" * 63, "a" * 64, "ab--cd",
    "xn--nxasmq6b", "xn--", "xn--a", "a_b", "a b",
    "münchen", "faß", "日本語", "é", "ü" * 63,
    "𐀀", "‮abc", "abc‮", "‍", "a‍b", "l·l", "l·l·l",
    "5③", "iii", "ß", "ﬁ", "Ａ", "﹒", "。", "a。b", "",
    NUL, "a" + DEL, "AAAAAAAA", "a-" * 40, "xn--a" * 20, "a." * 200,
    "̄abc", "abc̄", "A" * 64 + "B" * 64,
]
FLAGS = [
    (False, False, False, False),
    (True, False, False, False),
    (True, True, False, False),
    (True, True, True, False),
    (False, False, False, True),
]

FUZZ_DOMAINS = []
_rng = random.Random(20261029)
_alpha = [
    "m", "ü", "ß", "é", "日", "한", "م", "р",
    "̄", "̀", "‌", "‍", "·",
    "ς", "ֳ", "・", "٠", "۰", "ﬁ", "Ａ",
    "😀", "𐀀", ".", "-", "_", "。", "．", "｡",
]
for _ in range(1500):
    _n = _rng.randint(1, 4)
    _dom = ".".join(
        "".join(_rng.choice(_alpha) for _ in range(_rng.randint(1, 10)))
        for _ in range(_n)
    )
    FUZZ_DOMAINS.append(
        (_dom, _rng.random() < 0.8, _rng.random() < 0.3,
         _rng.random() < 0.2, _rng.random() < 0.2))


def _pure_encode(label, uts46, std3_rules, strict, display=False):
    """Pure-Python path with the kernel disabled."""
    import unittest.mock as mock

    with mock.patch.object(core, "_rust", None):
        return idna.encode(label, uts46=uts46, std3_rules=std3_rules, strict=strict)


def _pure_decode(label, uts46, std3_rules, strict, display=False):
    import unittest.mock as mock

    with mock.patch.object(core, "_rust", None):
        return idna.decode(label, uts46=uts46, std3_rules=std3_rules,
                           strict=strict, display=display)
def _outcome(fn, label, uts46, std3, strict, display):
    kw = dict(uts46=uts46, std3_rules=std3, strict=strict)
    if fn in (_pure_decode, idna.decode):
        kw["display"] = display
    try:
        return ("RET", repr(fn(label, **kw)))
    except idna.IDNAError as e:
        return ("ERR", type(e).__name__, e.code, str(e))


@pytest.mark.skipif(not HAVE_RUST, reason="no compiled extension")
@pytest.mark.parametrize("label", LABELS)
@pytest.mark.parametrize("uts46,std3,strict,display", FLAGS)
def test_kernel_matches_pure(label, uts46, std3, strict, display):
    assert _outcome(idna.encode, label, uts46, std3, strict, display) == \
        _outcome(_pure_encode, label, uts46, std3, strict, display)
    assert _outcome(idna.decode, label, uts46, std3, strict, display) == \
        _outcome(_pure_decode, label, uts46, std3, strict, display)


@pytest.mark.skipif(not HAVE_RUST, reason="no compiled extension")
@pytest.mark.parametrize("dom,uts46,std3,strict,display", FUZZ_DOMAINS)
def test_kernel_matches_pure_fuzz(dom, uts46, std3, strict, display):
    assert _outcome(idna.encode, dom, uts46, std3, strict, display) == \
        _outcome(_pure_encode, dom, uts46, std3, strict, display)
    assert _outcome(idna.decode, dom, uts46, std3, strict, display) == \
        _outcome(_pure_decode, dom, uts46, std3, strict, display)
