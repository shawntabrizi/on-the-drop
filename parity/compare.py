#!/usr/bin/env python3
"""Parity harness: diffs the GDScript dump (<name>.gd.json, <name>.bins) with
the Rust dump (<name>.<stage>.json, <name>.rs.bins) for every name in a dir.

  compare.py <dir> <stage>      stage: a (map from GDScript bins) or b (from PCM)

Prints one line per input and a summary; exit 1 when any field differs by
more than its tolerance. Tolerances: TOL_S for times (s), TOL_V for other
floats; ints, bytes, ids and words must be equal. The goal is 0 (bit equal).
"""
import json
import math
import os
import struct
import sys

TOL_S = 1e-4
TOL_V = 1e-4
TIMES = ["beats", "downbeats", "phrases", "drops", "kicks", "snares", "hats", "part_bounds", "finale"]
FLOATS = ["duration", "bpm", "kick_cover", "four_floor", "drop_step", "drop_strength", "grid_drift"]
EXACT = ["beat_bar", "beat_support", "energy", "kick_amp", "snare_amp", "hat_amp", "character",
         "words", "fixed_tempo", "levels", "bins"]


def read_bins(path):
    b = open(path, "rb").read()
    assert b[:8] == b"OTDBIN1\0"
    nb, bin_s, na = struct.unpack("<qdq", b[8:32])
    raw = struct.unpack("<%df" % (nb * 3), b[32:32 + nb * 12])
    air = struct.unpack("<%df" % na, b[32 + nb * 12:32 + nb * 12 + na * 4])
    return nb, bin_s, raw, air


def fdiff(a, b):
    """(count of unequal values, max abs diff); lengths must match."""
    if isinstance(a, (int, float)) and not isinstance(a, bool):
        a, b = [a], [b]
    if len(a) != len(b):
        return None
    n = 0
    mx = 0.0
    for x, y in zip(a, b):
        if x != y and not (isinstance(x, float) and math.isinf(x) and x == y):
            n += 1
            d = abs(x - y) if not (math.isinf(x) or math.isinf(y)) else math.inf
            mx = max(mx, d)
    return n, mx


def compare(g, r):
    """Returns {field: problem string} for this input."""
    bad = {}
    notes = {}
    for k in TIMES + FLOATS:
        if k not in g:
            continue
        d = fdiff(g[k], r[k])
        tol = TOL_S if k in TIMES else TOL_V
        if d is None:
            bad[k] = "length %d vs %d" % (len(g[k]), len(r[k]))
        elif d[0] > 0:
            (bad if d[1] > tol else notes)[k] = "%d differ, max %.3g" % d
    # sections: starts are times, ids exact
    gs, rs = g["sections"], r["sections"]
    if len(gs) != len(rs):
        bad["sections"] = "length %d vs %d" % (len(gs) // 2, len(rs) // 2)
    else:
        ds = fdiff(gs[0::2], rs[0::2])
        if ds[0] > 0:
            (bad if ds[1] > TOL_S else notes)["sections.start"] = "%d differ, max %.3g" % ds
        if gs[1::2] != rs[1::2]:
            bad["sections.id"] = "%s vs %s" % (gs[1::2], rs[1::2])
    for k in EXACT:
        if k in g and k in r and g[k] != r[k]:
            if isinstance(g[k], list) and len(g[k]) == len(r[k]):
                n = sum(1 for x, y in zip(g[k], r[k]) if x != y)
                bad[k] = "%d of %d differ" % (n, len(g[k]))
            else:
                bad[k] = "differ"
    return bad, notes


def main():
    d, stage = sys.argv[1], sys.argv[2]
    names = sorted(f[:-8] for f in os.listdir(d) if f.endswith(".gd.json"))
    n_bad = 0
    n = 0
    bins_bad = 0
    t_gd = t_rs = audio_s = 0.0
    tb_gd = tb_rs = 0.0
    for name in names:
        rj = os.path.join(d, "%s.%s.json" % (name, stage))
        if not os.path.exists(rj):
            continue
        g = json.load(open(os.path.join(d, name + ".gd.json")))
        r = json.load(open(rj))
        n += 1
        bad, notes = compare(g, r)
        if stage == "b":
            rb = os.path.join(d, name + ".rs.bins")
            gb = read_bins(os.path.join(d, name + ".bins"))
            xb = read_bins(rb)
            if gb[0] != xb[0] or gb[1] != xb[1]:
                bad["bins.shape"] = "nb %d vs %d, bin_s %r vs %r" % (gb[0], xb[0], gb[1], xb[1])
            else:
                for nm, i in (("bins.raw", 2), ("bins.air", 3)):
                    dd = fdiff(gb[i], xb[i])
                    if dd and dd[0] > 0:
                        bad[nm] = "%d differ, max %.3g" % dd
            if any(k.startswith("bins") for k in bad):
                bins_bad += 1
            if "ms_build" in g and "ms_decode" in g:
                tb_gd += g["ms_build"] - g["ms_decode"]
                tb_rs += r["ms_bins"] + r["ms_analyse"]
        t_gd += g.get("ms_analyse", 0.0)
        t_rs += r.get("ms_analyse", 0.0)
        audio_s += g["duration"]
        state = "OK  " if not bad else "DIFF"
        if bad:
            n_bad += 1
        extra = "; ".join("%s: %s" % kv for kv in sorted(bad.items()))
        if notes:
            extra += (" | " if extra else "") + "within tol: " + "; ".join("%s: %s" % kv for kv in sorted(notes.items()))
        print("%s %-60s %s" % (state, name[:60], extra))
    mins = audio_s / 60.0
    print("stage %s: %d inputs, %d with a difference beyond tolerance%s" % (
        stage, n, n_bad, (", %d with a bin difference" % bins_bad) if stage == "b" else ""))
    if mins > 0:
        print("map from bins: GDScript %.1f ms, Rust %.2f ms per audio minute (%.0fx)" % (
            t_gd / mins, t_rs / mins, t_gd / max(t_rs, 1e-9)))
        if tb_gd > 0:
            print("bins + map (decode excluded): GDScript %.1f ms, Rust %.2f ms per audio minute (%.0fx)" % (
                tb_gd / mins, tb_rs / mins, tb_gd / max(tb_rs, 1e-9)))
    sys.exit(1 if n_bad else 0)


if __name__ == "__main__":
    main()
