#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Turn the Criterion results of `benches/compare.rs` into tables.

  bench_report.py collect OUT.json
      Read target/criterion and save the median time (ns) of every benchmark.

  bench_report.py report [--out FILE] LABEL=RUN.json[,RUN.json...] ...
      Merge the runs of each machine (the minimum per cell, so one unlucky heap
      layout does not decide a row), print the scoreboard, and with --out write
      the markdown tables (BENCHMARKS.md).

`just compare` does both for the machine it runs on.
"""

import argparse
import json
import pathlib
import sys

SIZES = [256, 4096, 262144]
ORDER = [
    "count_ones", "first_set", "is_zero", "iter_ones_dense", "iter_ones_sparse",
    "and", "or", "xor", "get_batch1024", "write_batch1024", "set_range",
    "get_field24_batch1024", "reverse",
]
FUSED = "vbitmap (fused, a op b into dst)"
# `bitmap` and `compare` share Criterion's output directory. Keep collection
# scoped to the cases actually defined by `benches/compare.rs` so a previous
# run of the single-crate harness cannot add rows to the comparison report.
COMPARE_FUNCTIONS = {
    "count_ones": {"vbitmap", "fixedbitset", "bit-vec", "bit-set", "bitvec", "bitmaps"},
    "first_set": {
        "vbitmap", "fixedbitset", "bit-vec (iter().position)", "bit-set", "bitvec", "bitmaps",
    },
    "is_zero": {"vbitmap", "fixedbitset", "bit-vec", "bit-set", "bitvec", "bitmaps"},
    "iter_ones_dense": {
        "vbitmap", "fixedbitset", "bit-vec (iter+filter)", "bit-set", "bitvec", "bitmaps",
    },
    "iter_ones_sparse": {
        "vbitmap", "fixedbitset", "bit-vec (iter+filter)", "bit-set", "bitvec", "bitmaps",
    },
    "and": {
        "vbitmap", FUSED, "fixedbitset", "bit-vec", "bit-set", "bitvec",
    },
    "or": {
        "vbitmap", "vbitmap (fused, a op b into dst)",
        "fixedbitset", "bit-vec", "bit-set", "bitvec",
    },
    "xor": {
        "vbitmap", "vbitmap (fused, a op b into dst)",
        "fixedbitset", "bit-vec", "bit-set", "bitvec",
    },
    "get_batch1024": {"vbitmap", "fixedbitset", "bit-vec", "bit-set", "bitvec", "bv"},
    "write_batch1024": {
        "vbitmap (set)", "vbitmap (BitSlice set/unset)", "fixedbitset", "bit-vec",
        "bit-set", "bitvec", "bv",
    },
    "set_range": {"vbitmap", "fixedbitset", "bitvec (slice.fill)"},
    "get_field24_batch1024": {"vbitmap", "bitvec (load_le)", "bv (get_bits)"},
    "reverse": {"vbitmap", "bitvec"},
}
TITLES = {
    "count_ones": "Population count",
    "first_set": "First set bit (worst case: only the last bit is set)",
    "is_zero": "Is the bitmap all zero",
    "iter_ones_dense": "Iterate set bits, half of the bits set",
    "iter_ones_sparse": "Iterate set bits, 1 in 64 set",
    "and": "In-place AND (clone the left operand, then AND)",
    "or": "In-place OR (clone, then OR)",
    "xor": "In-place XOR (clone, then XOR)",
    "get_batch1024": "1024 random bit reads",
    "write_batch1024": "1024 random bit writes (alternating a set pass and a clear pass)",
    "set_range": "Set or clear a range of bits",
    "get_field24_batch1024": "1024 random 24-bit field reads",
    "reverse": "Reverse the order of all stored bits",
}
NOTES = {
    "first_set": "`bit-vec` has no search, so it is measured as `iter().position(..)`.",
    "iter_ones_dense": "`bit-vec` has no set-bit iterator, so it is measured as `iter().enumerate().filter(..)`.",
    "iter_ones_sparse": "`bit-vec` has no set-bit iterator, so it is measured as `iter().enumerate().filter(..)`.",
    "write_batch1024": (
        "The reference is `vbitmap (BitSlice set/unset)` (called on a `BitSlice` "
        "borrowed once, as a hot loop would): bounds-checked, never grows, like the "
        "`set` of `fixedbitset`, `bit-vec`, `bitvec` and `bv`, which all panic on an "
        "out-of-range index. `vbitmap (set)` grows the bitmap on demand and is the "
        "like-for-like row for `bit-set`, which also grows. Alternating "
        "passes make every crate really store: `bit-set`'s `insert`/`remove` skip the "
        "store when the bit already has the value, so repeating the same write would "
        "flatter it."
    ),
}
for _op, _sym in (("and", "&"), ("or", "|"), ("xor", "^")):
    NOTES[_op] = (
        f"`vbitmap` is clone then in-place {_op.upper()}, like every other crate. "
        f"`vbitmap (fused)` computes `dst = a {_sym} b` in one pass (`{_op}_from`), "
        "which no other crate offers; the reference for the ratios is the faster of the two."
    )
# Rows the ratios are computed against (default: the plain `vbitmap` row).
REFERENCE = {op: ["vbitmap", FUSED] for op in ("and", "or", "xor")}
REFERENCE["write_batch1024"] = ["vbitmap (BitSlice set/unset)"]
# Not dense bitmaps: they do not decide who is fastest.
NOT_DENSE = {"bitmaps"}


def collect(out):
    res = {}
    root = pathlib.Path("target/criterion")
    for bench in root.glob("*/*/*/new/benchmark.json"):
        info = json.loads(bench.read_text())
        if info["function_id"] not in COMPARE_FUNCTIONS.get(info["group_id"], set()):
            continue
        est = json.loads((bench.parent / "estimates.json").read_text())
        res[f"{info['group_id']}|{info['function_id']}|{info['value_str']}"] = est["median"]["point_estimate"]
    pathlib.Path(out).write_text(json.dumps(res, indent=1, sort_keys=True))
    print(f"{len(res)} benchmarks -> {out}")


def merge(files):
    """{op: {crate: {size: ns}}}, minimum over the runs."""
    data = {}
    for f in files:
        for key, ns in json.loads(pathlib.Path(f).read_text()).items():
            op, crate, size = key.split("|")
            cell = data.setdefault(op, {}).setdefault(crate, {})
            cell[int(size)] = min(ns, cell.get(int(size), ns))
    return data


def reference(op, crates):
    rows = REFERENCE.get(op, ["vbitmap"])
    rows = [r for r in rows if r in crates]
    return rows, {s: min(crates[r][s] for r in rows) for s in SIZES}


def scoreboard(label, data):
    ahead = level = mid = slow = 0
    cells = []
    for op in ORDER:
        crates = data[op]
        _, ref = reference(op, crates)
        for s in SIZES:
            rivals = {
                c: v[s] for c, v in crates.items()
                if not c.startswith("vbitmap") and c.split(" ")[0] not in NOT_DENSE and s in v
            }
            who = min(rivals, key=rivals.get)
            ratio = rivals[who] / ref[s]
            if ratio >= 1.05:
                ahead += 1
            elif ratio >= 0.95:
                level += 1
            elif ratio >= 0.90:
                mid += 1
            else:
                slow += 1
            if ratio < 1.05:
                cells.append((round(ratio, 2), op, s, who))
    n = ahead + level + mid + slow
    print(f"{label}: {n} cells, clearly ahead {ahead}, within 5% {level}, "
          f"5-10% slower {mid}, more than 10% slower {slow}")
    for c in sorted(cells):
        print("   ", c)


def fmt(ns):
    return f"{ns:.1f}" if ns < 100 else f"{ns:,.0f}"


HEADER = """# Benchmarks

`vbitmap` against the most downloaded bitmap and bitset crates on crates.io
(by downloads, search "bitmap" and "bitset": `fixedbitset`, `bit-vec`, `bit-set`,
`bitvec`, `bitmaps`, `bv`). Same data in every crate, same operations,
one Criterion harness: `benches/compare.rs` (run it with `just compare`).

## How to read the tables

Each cell is the median time of one call in nanoseconds. The ratio in parentheses
is that crate's time divided by vbitmap's, so above 1.00 means slower than vbitmap;
compare ratios across machines, not nanoseconds.

* **Result:** of {total} measurements ({ops} operations, 3 sizes, {machines} machines)
  vbitmap is clearly ahead of the best dense crate in {ahead} and level with it in
  {level} (within about 5%); {behind}.
* **Method:** Criterion medians, and the minimum over {runs} runs with different
  heap layouts (`LAYOUT_SHIFT`). Differences under about 5% are ties.
* **Placement noise:** rows of a few nanoseconds move 10 to 30% with where the
  compiler puts a loop or the allocator puts a small bitmap, for any crate.
* **Not like for like:** `bitmaps` supports fixed-size bitmaps up to 1024 bits.
  Of our benchmark sizes (256, 4096 and 262144 bits), only 256 fits; the larger
  sizes are marked `n/a`.
  `bit-vec` has no search or set-bit iterator (measured as `iter().position(..)`
  and `iter().enumerate().filter(..)`). `bit-vec` and `bit-set` use `usize`
  blocks (their default, `u32`, is slower).
* Some vbitmap fast paths are per architecture (see the README).

## Machines

{machines_list}
"""


def report(args):
    machines = []
    for spec in args.machines:
        label, files = spec.rsplit("=", 1)
        machines.append((label, files.split(",")))
    results = [(label, merge(files)) for label, files in machines]
    for label, data in results:
        scoreboard(label, data)
    if not args.out:
        return
    total = ahead = level = behind = 0
    for _, data in results:
        for op in ORDER:
            crates = data[op]
            _, ref = reference(op, crates)
            for s in SIZES:
                best = min(v[s] for c, v in crates.items()
                           if not c.startswith("vbitmap") and c.split(" ")[0] not in NOT_DENSE and s in v)
                r = best / ref[s]
                total += 1
                ahead += r >= 1.05
                level += 0.95 <= r < 1.05
                behind += r < 0.95
    out = [HEADER.format(
        total=total, ops=len(ORDER), machines=len(results), ahead=ahead, level=level,
        behind="it is never meaningfully behind" if behind == 0 else f"{behind} cells are behind by more than 5%",
        runs=max(len(f) for _, f in machines),
        machines_list="\n".join(f"* {label}" for label, _ in machines),
    )]
    for label, data in results:
        out.append(f"\n## {label.split(',')[0]}\n")
        for op in ORDER:
            crates = data[op]
            rows, ref = reference(op, crates)
            out.append(f"### {TITLES[op]}\n")
            if op in NOTES:
                out.append(NOTES[op] + "\n")
            out.append("| crate (ns per call) | 256 bits | 4096 bits | 262144 bits |\n|---|---|---|---|")
            for name in sorted(crates, key=lambda c: (not c.startswith("vbitmap"), c)):
                cells = []
                for s in SIZES:
                    v = crates[name].get(s)
                    if v is None:
                        cells.append("n/a")
                    elif name in rows and v == ref[s]:
                        cells.append(f"**{fmt(v)}**")
                    else:
                        cells.append(f"{fmt(v)} ({v / ref[s]:.2f})")
                out.append(f"| {name} | " + " | ".join(cells) + " |")
            out.append("")
    pathlib.Path(args.out).write_text("\n".join(out))
    print(f"wrote {args.out}")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("collect")
    c.add_argument("out")
    r = sub.add_parser("report")
    r.add_argument("--out")
    r.add_argument("machines", nargs="+", metavar="LABEL=RUN.json[,RUN.json...]")
    args = p.parse_args()
    if args.cmd == "collect":
        collect(args.out)
    else:
        report(args)


if __name__ == "__main__":
    sys.exit(main())
