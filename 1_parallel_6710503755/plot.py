#!/usr/bin/env python3
"""plot.py - turn the CSV files written by bench.sh into graphs and an
Amdahl's-law summary for the report.

Educational script for an Operating Systems course report.

Usage:  python3 plot.py [RESULTS_DIR]        (default: ./results)
Reads   RESULTS_DIR/raw/*.csv
Writes  RESULTS_DIR/figures/*.png and RESULTS_DIR/summary.md
Needs   matplotlib (no other third-party package).
"""

import csv
import math
import os
import statistics
import sys
from collections import defaultdict

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

# ------------------------------------------------------------------ style
SURFACE = "#fcfcfb"
TEXT = "#0b0b0b"
TEXT2 = "#52514e"
GRID = "#e4e3df"
NEUTRAL = "#8a8984"
# Categorical slots, always assigned in this fixed order.
CAT = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"]
# Ordinal blue ramp (light -> dark) for ordered values such as n.
BLUE_RAMP = ["#86b6ef", "#5598e7", "#2a78d6", "#1c5cab", "#104281", "#0d366b"]
MARKERS = ["o", "s", "^", "D", "v", "P", "X", "*"]
ALGO_ORDER = ["naive", "ikj", "transpose", "blocked"]
ALGO_COLOR = {a: CAT[i] for i, a in enumerate(ALGO_ORDER)}
ALGO_MARKER = {a: MARKERS[i] for i, a in enumerate(ALGO_ORDER)}

plt.rcParams.update({
    "figure.facecolor": SURFACE,
    "axes.facecolor": SURFACE,
    "savefig.facecolor": SURFACE,
    "axes.edgecolor": GRID,
    "axes.labelcolor": TEXT2,
    "axes.titlecolor": TEXT,
    "axes.titlesize": 11,
    "axes.labelsize": 10,
    "axes.grid": True,
    "grid.color": GRID,
    "grid.linewidth": 0.8,
    "grid.linestyle": "-",
    "axes.spines.top": False,
    "axes.spines.right": False,
    "xtick.color": TEXT2,
    "ytick.color": TEXT2,
    "legend.frameon": False,
    "legend.fontsize": 8.5,
    "lines.linewidth": 2,
    "lines.markersize": 6,
    "lines.solid_capstyle": "round",
    "lines.solid_joinstyle": "round",
    "font.size": 10,
    "figure.dpi": 110,
    "savefig.dpi": 200,
})


def mstyle(color, marker):
    """Line + marker with a surface-coloured ring so markers stay legible."""
    return dict(color=color, marker=marker, markeredgecolor=SURFACE, markeredgewidth=1.5)


# ------------------------------------------------------------------ data
NUM_FIELDS = {
    "n": int, "threads": int, "chunk": int, "block": int, "verified": int,
    "t_init": float, "t_mul_best": float, "t_mul_mean": float, "t_post": float,
    "t_verify": float, "t_total": float, "gflops": float, "mem_theory_mb": float,
    "maxrss_mb": float, "vmpeak_mb": float, "checksum": float,
    "time_maxrss_kb": float,
}


def load(raw, name):
    path = os.path.join(raw, name)
    if not os.path.exists(path):
        return []
    rows = []
    with open(path, newline="") as f:
        for r in csv.DictReader(f):
            for k, conv in NUM_FIELDS.items():
                if k in r and r[k] not in (None, ""):
                    r[k] = conv(r[k])
            rows.append(r)
    return rows


def where(rows, **cond):
    return [r for r in rows if all(r.get(k) == v for k, v in cond.items())]


def one(rows, **cond):
    m = where(rows, **cond)
    return m[0] if m else None


def label_kv(label):
    """'bind=close;places=cores' -> {'bind': 'close', 'places': 'cores'}"""
    out = {}
    for part in label.split(";"):
        if "=" in part:
            k, v = part.split("=", 1)
            out[k] = v
    return out


def program_time(r):
    """One complete run of the program: serial init + one kernel + serial post."""
    return r["t_init"] + r["t_mul_mean"] + r["t_post"]


def save(fig, figdir, name):
    fig.tight_layout()
    path = os.path.join(figdir, name)
    fig.savefig(path)
    plt.close(fig)
    print("  wrote", path)


def log2_threads_axis(ax, threads):
    ax.set_xscale("log", base=2)
    ax.set_xticks(threads)
    ax.set_xticklabels([str(t) for t in threads])
    ax.minorticks_off()


def shade_smt(ax, ncore, tmax):
    """Shade the region where threads > physical cores (SMT siblings share a core)."""
    if tmax > ncore:
        ax.axvspan(ncore, tmax * 1.05, color=GRID, alpha=0.45, lw=0, zorder=0)
        ax.text(ncore * 1.04, 0.97, "SMT\n(> %d cores)" % ncore, transform=ax.get_xaxis_transform(),
                va="top", ha="left", fontsize=7.5, color=TEXT2)


# ------------------------------------------------------------------ Amdahl
def fit_amdahl(ps, speedups):
    """Least-squares fit of S(p) = 1 / ((1-f) + f/p).
    Rearranged: 1 - 1/S = f * (1 - 1/p), i.e. y = f*x through the origin."""
    xs = [1 - 1 / p for p in ps]
    ys = [1 - 1 / s for s in speedups]
    sxx = sum(x * x for x in xs)
    if sxx == 0:
        return float("nan")
    f = sum(x * y for x, y in zip(xs, ys)) / sxx
    return min(max(f, 0.0), 1.0)


def amdahl(f, p):
    return 1.0 / ((1 - f) + f / p)


def karp_flatt(s, p):
    """Experimentally determined serial fraction e = (1/S - 1/p) / (1 - 1/p)."""
    return (1 / s - 1 / p) / (1 - 1 / p) if p > 1 else float("nan")


# ------------------------------------------------------------------ figures
def fig_scaling(scaling, figdir, ncore):
    algos = [a for a in ALGO_ORDER if where(scaling, algo=a)]
    sizes = sorted({r["n"] for r in scaling})
    threads = sorted({r["threads"] for r in scaling})
    ramp = BLUE_RAMP[-len(sizes):] if len(sizes) <= len(BLUE_RAMP) else BLUE_RAMP

    for kind in ("speedup", "efficiency"):
        fig, axes = plt.subplots(1, len(algos), figsize=(4.2 * len(algos), 3.9), sharey=True)
        axes = axes if len(algos) > 1 else [axes]
        for ax, a in zip(axes, algos):
            if kind == "speedup":
                ax.plot(threads, threads, color=NEUTRAL, lw=1.2, ls=(0, (4, 3)), label="ideal (S = p)")
            else:
                ax.axhline(1.0, color=NEUTRAL, lw=1.2, ls=(0, (4, 3)), label="ideal (E = 1)")
            for i, n in enumerate(sizes):
                base = one(scaling, algo=a, n=n, threads=1)
                if base is None:
                    continue
                pts = sorted(where(scaling, algo=a, n=n), key=lambda r: r["threads"])
                ps = [r["threads"] for r in pts]
                s = [base["t_mul_best"] / r["t_mul_best"] for r in pts]
                y = s if kind == "speedup" else [si / p for si, p in zip(s, ps)]
                ax.plot(ps, y, label="n = %d" % n, **mstyle(ramp[i % len(ramp)], MARKERS[i % len(MARKERS)]))
            log2_threads_axis(ax, threads)
            shade_smt(ax, ncore, max(threads))
            ax.set_title(a)
            ax.set_xlabel("threads p")
        axes[0].set_ylabel("speedup  T(1) / T(p)" if kind == "speedup" else "efficiency  S(p) / p")
        axes[-1].legend(loc="upper left", bbox_to_anchor=(1.0, 1.0))
        fig.suptitle("Kernel %s vs thread count (one line per matrix size)" % kind, color=TEXT, fontsize=12)
        save(fig, figdir, "fig2_speedup.png" if kind == "speedup" else "fig3_efficiency.png")

    # Absolute time vs threads, for the largest n of each algorithm.
    fig, axes = plt.subplots(1, len(algos), figsize=(4.2 * len(algos), 3.9))
    axes = axes if len(algos) > 1 else [axes]
    for ax, a in zip(axes, algos):
        for i, n in enumerate(sizes):
            pts = sorted(where(scaling, algo=a, n=n), key=lambda r: r["threads"])
            if not pts:
                continue
            ax.plot([r["threads"] for r in pts], [r["t_mul_best"] for r in pts],
                    label="n = %d" % n, **mstyle(ramp[i % len(ramp)], MARKERS[i % len(MARKERS)]))
        ax.set_yscale("log")
        log2_threads_axis(ax, threads)
        shade_smt(ax, ncore, max(threads))
        ax.set_title(a)
        ax.set_xlabel("threads p")
    axes[0].set_ylabel("kernel time (s, log scale)")
    axes[-1].legend(loc="upper left", bbox_to_anchor=(1.0, 1.0))
    fig.suptitle("Kernel time vs thread count", color=TEXT, fontsize=12)
    save(fig, figdir, "fig1b_time_vs_threads.png")


def fig_algos(algos_rows, figdir):
    tcounts = sorted({r["threads"] for r in algos_rows})
    sizes = sorted({r["n"] for r in algos_rows})
    algos = [a for a in ALGO_ORDER if where(algos_rows, algo=a)]

    for metric, fname, ylabel, title in (
        ("t_mul_best", "fig1_time_vs_n.png", "kernel time (s, log scale)", "Kernel time vs matrix size"),
        ("gflops", "fig4_gflops_vs_n.png", "GFLOP/s  (2n³ / time)", "Throughput vs matrix size"),
    ):
        fig, axes = plt.subplots(1, len(tcounts), figsize=(4.2 * len(tcounts), 3.9), sharey=True)
        axes = axes if len(tcounts) > 1 else [axes]
        for ax, t in zip(axes, tcounts):
            for a in algos:
                pts = sorted(where(algos_rows, algo=a, threads=t), key=lambda r: r["n"])
                ax.plot([r["n"] for r in pts], [r[metric] for r in pts], label=a,
                        **mstyle(ALGO_COLOR[a], ALGO_MARKER[a]))
            if metric == "t_mul_best":
                ref = [r for r in where(algos_rows, algo="ikj", threads=t)]
                if ref:
                    r0 = min(ref, key=lambda r: abs(r["n"] - 800))
                    xs = sizes
                    ax.plot(xs, [r0["t_mul_best"] * (x / r0["n"]) ** 3 for x in xs], color=NEUTRAL,
                            lw=1, ls=(0, (1, 2)), label="∝ n³ (reference)")
                ax.set_yscale("log")
            ax.set_xscale("log", base=2)
            ax.set_xticks(sizes)
            ax.set_xticklabels([str(s) for s in sizes])
            ax.minorticks_off()
            ax.set_title("%d thread%s" % (t, "" if t == 1 else "s"))
            ax.set_xlabel("matrix size n")
        axes[0].set_ylabel(ylabel)
        axes[-1].legend(loc="upper left", bbox_to_anchor=(1.0, 1.0))
        fig.suptitle(title + " (4 algorithms)", color=TEXT, fontsize=12)
        save(fig, figdir, fname)


def fig_sched(sched, figdir):
    if not sched:
        return
    scheds = ["static", "dynamic", "guided"]
    chunks = sorted({r["chunk"] for r in sched})
    combos = sorted({(r["n"], r["threads"]) for r in sched})
    fig, axes = plt.subplots(1, len(combos), figsize=(3.6 * len(combos), 3.2))
    axes = axes if len(combos) > 1 else [axes]
    from matplotlib.colors import LinearSegmentedColormap
    cmap = LinearSegmentedColormap.from_list("blue", ["#cde2fb", "#86b6ef", "#2a78d6", "#184f95", "#0d366b"])
    for ax, (n, t) in zip(axes, combos):
        best = min(r["t_mul_best"] for r in where(sched, n=n, threads=t))
        grid = []
        for s in scheds:
            row = []
            for c in chunks:
                r = one(sched, n=n, threads=t, schedule=s, chunk=c)
                row.append(r["t_mul_best"] / best if r else float("nan"))
            grid.append(row)
        im = ax.imshow(grid, cmap=cmap, aspect="auto", vmin=1.0, vmax=max(max(g) for g in grid))
        for i in range(len(scheds)):
            for j in range(len(chunks)):
                v = grid[i][j]
                ax.text(j, i, "%.2f" % v, ha="center", va="center", fontsize=8,
                        color="white" if v > 1 + 0.55 * (im.norm.vmax - 1) else TEXT)
        ax.set_xticks(range(len(chunks)))
        ax.set_xticklabels(["default" if c == 0 else str(c) for c in chunks])
        ax.set_yticks(range(len(scheds)))
        ax.set_yticklabels(scheds)
        ax.grid(False)
        ax.set_xlabel("chunk size")
        ax.set_title("n=%d, %d threads" % (n, t))
    fig.suptitle("OpenMP schedule: kernel time relative to best (1.00 = fastest), algo ikj",
                 color=TEXT, fontsize=11)
    save(fig, figdir, "fig5_schedule.png")


def fig_block(block, figdir):
    if not block:
        return
    fig, ax = plt.subplots(figsize=(6, 3.8))
    for i, t in enumerate(sorted({r["threads"] for r in block})):
        pts = sorted(where(block, threads=t), key=lambda r: r["block"])
        ax.plot([r["block"] for r in pts], [r["gflops"] for r in pts], label="%d thread%s" % (t, "" if t == 1 else "s"),
                **mstyle(CAT[i], MARKERS[i]))
    bs = sorted({r["block"] for r in block})
    ax.set_xscale("log", base=2)
    ax.set_xticks(bs)
    ax.set_xticklabels([str(b) for b in bs])
    ax.minorticks_off()
    ax.set_xlabel("block size (bs × bs tile of doubles)")
    ax.set_ylabel("GFLOP/s")
    ax.set_title("Blocked algorithm: effect of block size (n = %d)" % block[0]["n"])
    ax.legend()
    save(fig, figdir, "fig6_block_size.png")


def grouped_bars(ax, groups, series, value, colors, fmt="%.3g"):
    """groups: x categories; series: list of (name, {group: value}).
    Value labels are drawn only when there are <= 3 series (otherwise too crowded;
    the exact numbers are in summary.md)."""
    k = len(series)
    width = min(0.8 / k, 0.28)
    for i, (name, vals) in enumerate(series):
        xs = [g + (i - (k - 1) / 2) * (width + 0.02) for g in range(len(groups))]
        ys = [vals.get(g, float("nan")) for g in groups]
        bars = ax.bar(xs, ys, width=width, color=colors[i], label=name, zorder=2)
        for b, y in zip(bars, ys):
            if k <= 3 and not math.isnan(y):
                ax.text(b.get_x() + b.get_width() / 2, y, fmt % y, ha="center", va="bottom",
                        fontsize=7, color=TEXT2)
    ax.set_xticks(range(len(groups)))
    ax.set_xticklabels(groups)
    ax.grid(axis="x", visible=False)


def fig_flags(flags, figdir):
    if not flags:
        return
    variants = ["O0", "O2", "O3native"]
    tcounts = sorted({r["threads"] for r in flags})
    algos = [a for a in ALGO_ORDER if where(flags, algo=a)]
    fig, axes = plt.subplots(1, len(tcounts), figsize=(5 * len(tcounts), 3.8), sharey=True)
    axes = axes if len(tcounts) > 1 else [axes]
    for ax, t in zip(axes, tcounts):
        series = []
        for v in variants:
            vals = {}
            for r in where(flags, threads=t):
                if label_kv(r["label"]).get("flags") == v:
                    vals[r["algo"]] = r["gflops"]
            series.append(({"O0": "-O0", "O2": "-O2", "O3native": "-O3 -march=native"}[v], vals))
        grouped_bars(ax, algos, series, "gflops", CAT[:3], fmt="%.1f")
        ax.set_title("%d thread%s" % (t, "" if t == 1 else "s"))
    axes[0].set_ylabel("GFLOP/s")
    axes[-1].legend(loc="upper left", bbox_to_anchor=(1.0, 1.0))
    fig.suptitle("Compiler optimisation level (-O0 measured at n=800, others at n=%d)"
                 % max(r["n"] for r in flags), color=TEXT, fontsize=11)
    save(fig, figdir, "fig7_compiler_flags.png")


def fig_bind(bind, figdir):
    if not bind:
        return
    configs = []
    for r in bind:
        if r["label"] not in configs:
            configs.append(r["label"])
    tcounts = sorted({r["threads"] for r in bind})
    algos = [a for a in ALGO_ORDER if where(bind, algo=a)]
    fig, axes = plt.subplots(1, len(algos), figsize=(5.5 * len(algos), 3.9), sharey=False)
    axes = axes if len(algos) > 1 else [axes]
    for ax, a in zip(axes, algos):
        series = []
        for c in configs:
            kv = label_kv(c)
            name = "unbound" if kv.get("bind") == "false" else "%s / %s" % (kv.get("bind"), kv.get("places"))
            series.append((name, {"%d thr" % r["threads"]: r["t_mul_best"] for r in where(bind, algo=a, label=c)}))
        grouped_bars(ax, ["%d thr" % t for t in tcounts], series, "t", CAT[:len(series)])
        ax.set_title(a)
        ax.set_ylabel("kernel time (s), lower is better")
    axes[-1].legend(title="OMP_PROC_BIND / OMP_PLACES", loc="upper left", bbox_to_anchor=(1.0, 1.0),
                    title_fontsize=8)
    fig.suptitle("Thread placement (n = %d)" % bind[0]["n"], color=TEXT, fontsize=12)
    save(fig, figdir, "fig8_binding.png")


def fig_affinity_nice(aff, nice, figdir):
    if not aff and not nice:
        return
    fig, axes = plt.subplots(1, 2, figsize=(11, 3.9))
    if aff:
        ax = axes[0]
        masks = []
        for r in aff:
            m = label_kv(r["label"])["cpus"]
            if m not in masks:
                masks.append(m)
        series = []
        for t in sorted({r["threads"] for r in aff}):
            series.append(("%d thread%s" % (t, "" if t == 1 else "s"),
                           {label_kv(r["label"])["cpus"]: r["t_mul_best"] for r in where(aff, threads=t)}))
        grouped_bars(ax, masks, series, "t", CAT[:len(series)])
        ax.set_xlabel("allowed CPUs (taskset -c)")
        ax.set_ylabel("kernel time (s), lower is better")
        ax.set_title("CPU affinity mask / oversubscription (ikj)", pad=26)
        ax.legend(loc="lower center", bbox_to_anchor=(0.5, 1.0), ncol=len(series), fontsize=8)
    if nice:
        ax = axes[1]
        groups = []
        for r in nice:
            kv = label_kv(r["label"])
            g = "%s load\nnice %s" % ("no" if kv["load"] == "none" else "busy", kv["nice"])
            if g not in groups:
                groups.append(g)
        # Dot plot (not bars): the range spans orders of magnitude, so a log axis
        # is needed, and bar lengths on a log axis would be meaningless.
        tcounts = sorted({r["threads"] for r in nice})
        for i, t in enumerate(tcounts):
            xs, ys = [], []
            for r in where(nice, threads=t):
                kv = label_kv(r["label"])
                xs.append(groups.index("%s load\nnice %s" % ("no" if kv["load"] == "none" else "busy", kv["nice"]))
                          + (i - (len(tcounts) - 1) / 2) * 0.3)
                ys.append(r["t_mul_best"])
            ax.plot(xs, ys, ls="none", markersize=9, label="%d thread%s" % (t, "" if t == 1 else "s"),
                    **mstyle(CAT[i], MARKERS[i]))
            for x, y in zip(xs, ys):
                ax.annotate("%.3g" % y, (x, y), textcoords="offset points", xytext=(0, 8),
                            ha="center", fontsize=7, color=TEXT2)
        ax.set_xticks(range(len(groups)))
        ax.set_xticklabels(groups)
        ax.set_xlim(-0.6, len(groups) - 0.4)
        ax.set_yscale("log")
        ax.grid(axis="x", visible=False)
        ax.set_ylabel("kernel time (s, log), lower is better")
        ax.set_title("Scheduler priority under competing CPU load (ikj)", pad=26)
        ax.legend(loc="lower center", bbox_to_anchor=(0.5, 1.0), ncol=len(tcounts), fontsize=8)
    save(fig, figdir, "fig9_affinity_nice.png")


def fig_memory(mem, stack, figdir):
    if not mem:
        return
    fig, axes = plt.subplots(1, 2, figsize=(11, 3.9))
    ax = axes[0]
    sizes = sorted({r["n"] for r in mem})
    i = 0
    for a in [x for x in ALGO_ORDER if where(mem, algo=x)]:
        for t in sorted({r["threads"] for r in mem}):
            pts = sorted(where(mem, algo=a, threads=t), key=lambda r: r["n"])
            ax.plot([r["n"] for r in pts], [r["maxrss_mb"] for r in pts],
                    label="%s, %d thr (peak RSS)" % (a, t), **mstyle(CAT[i], MARKERS[i]))
            i += 1
        pts = sorted(where(mem, algo=a, threads=1), key=lambda r: r["n"])
        ax.plot([r["n"] for r in pts], [r["mem_theory_mb"] for r in pts], color=NEUTRAL, lw=1,
                ls=(0, (1, 2)) if a == "ikj" else (0, (4, 2)),
                label="%s theory (%d·n²·8 B)" % (a, 4 if a == "transpose" else 3))
    ax.set_xscale("log", base=2)
    ax.set_yscale("log")
    ax.set_xticks(sizes)
    ax.set_xticklabels([str(s) for s in sizes])
    ax.minorticks_off()
    ax.set_xlabel("matrix size n")
    ax.set_ylabel("memory (MiB, log)")
    ax.set_title("Physical memory used vs n")
    ax.legend(fontsize=7.5)

    ax = axes[1]
    if stack:
        for i, ss in enumerate(["default", "1M", "64M"]):
            pts = sorted([r for r in stack if label_kv(r["label"])["stack"] == ss], key=lambda r: r["threads"])
            if not pts:
                continue
            name = "OMP_STACKSIZE=%s" % ss if ss != "default" else "default stack"
            ax.plot([r["threads"] for r in pts], [r["vmpeak_mb"] for r in pts], label=name + " (VmPeak)",
                    **mstyle(CAT[i], MARKERS[i]))
        pts = sorted([r for r in stack if label_kv(r["label"])["stack"] == "default"], key=lambda r: r["threads"])
        ax.plot([r["threads"] for r in pts], [r["maxrss_mb"] for r in pts], label="peak RSS (all settings)",
                **mstyle(NEUTRAL, "o"))
        ax.set_yscale("log")
        ax.set_xlabel("threads")
        ax.set_ylabel("MiB (log)")
        ax.set_title("Per-thread overhead: virtual vs physical (n = 100)")
        ax.legend(fontsize=7.5)
    save(fig, figdir, "fig11_memory.png")


def fig_amdahl(scaling, figdir, ncore, fits):
    algos = [a for a in ALGO_ORDER if where(scaling, algo=a)]
    sizes = sorted({r["n"] for r in scaling})
    n_show = max(n for n in sizes if all(one(scaling, algo=a, n=n, threads=1) for a in algos)) \
        if any(all(one(scaling, algo=a, n=n, threads=1) for a in algos) for n in sizes) else sizes[-1]
    threads = sorted({r["threads"] for r in scaling})
    fig, axes = plt.subplots(1, len(algos) + 1, figsize=(4.0 * (len(algos) + 1), 3.9))
    for ax, a in zip(axes, algos):
        base = one(scaling, algo=a, n=n_show, threads=1)
        pts = sorted(where(scaling, algo=a, n=n_show), key=lambda r: r["threads"])
        ps = [r["threads"] for r in pts]
        sk = [base["t_mul_best"] / r["t_mul_best"] for r in pts]
        sp = [program_time(base) / program_time(r) for r in pts]
        fk = fits[(a, n_show)]["f_kernel"]
        fp = fits[(a, n_show)]["f_program"]
        pp = [1 + i * 0.25 for i in range(int((max(ps) - 1) / 0.25) + 1)]
        ax.plot(ps, ps, color=NEUTRAL, lw=1, ls=(0, (4, 3)), label="ideal")
        ax.plot(ps, sk, ls="none", label="measured kernel", **mstyle(CAT[0], "o"))
        ax.plot(pp, [amdahl(fk, p) for p in pp], color=CAT[0], lw=1.5, label="Amdahl fit f=%.4f" % fk)
        ax.plot(ps, sp, ls="none", label="measured program", **mstyle(CAT[1], "s"))
        ax.plot(pp, [amdahl(fp, p) for p in pp], color=CAT[1], lw=1.5, label="Amdahl fit f=%.4f" % fp)
        log2_threads_axis(ax, threads)
        shade_smt(ax, ncore, max(threads))
        ax.set_title("%s, n = %d" % (a, n_show))
        ax.set_xlabel("threads p")
        ax.legend(fontsize=7)
    axes[0].set_ylabel("speedup")

    ax = axes[-1]
    for i, a in enumerate(algos):
        base = one(scaling, algo=a, n=n_show, threads=1)
        pts = sorted([r for r in where(scaling, algo=a, n=n_show) if r["threads"] > 1], key=lambda r: r["threads"])
        ax.plot([r["threads"] for r in pts], [karp_flatt(base["t_mul_best"] / r["t_mul_best"], r["threads"]) for r in pts],
                label=a, **mstyle(ALGO_COLOR[a], ALGO_MARKER[a]))
    log2_threads_axis(ax, [t for t in threads if t > 1])
    shade_smt(ax, ncore, max(threads))
    ax.set_title("Karp–Flatt serial fraction e(p), kernel")
    ax.set_xlabel("threads p")
    ax.set_ylabel("e(p)")
    ax.legend(fontsize=7.5)
    fig.suptitle("Amdahl's law fit (fit uses p ≤ %d physical cores)" % ncore, color=TEXT, fontsize=12)
    save(fig, figdir, "fig12_amdahl.png")


# ------------------------------------------------------------------ summary
def md_table(header, rows):
    out = ["| " + " | ".join(header) + " |", "|" + "|".join("---" for _ in header) + "|"]
    for r in rows:
        out.append("| " + " | ".join(str(x) for x in r) + " |")
    return "\n".join(out)


def amdahl_fits(scaling, ncore):
    fits = {}
    for a in ALGO_ORDER:
        for n in sorted({r["n"] for r in where(scaling, algo=a)}):
            base = one(scaling, algo=a, n=n, threads=1)
            if base is None:
                continue
            pts = [r for r in where(scaling, algo=a, n=n) if 1 < r["threads"] <= ncore]
            if not pts:
                continue
            ps = [r["threads"] for r in pts]
            sk = [base["t_mul_best"] / r["t_mul_best"] for r in pts]
            sp = [program_time(base) / program_time(r) for r in pts]
            allpts = where(scaling, algo=a, n=n)
            best = max(allpts, key=lambda r: base["t_mul_best"] / r["t_mul_best"])
            fk, fp = fit_amdahl(ps, sk), fit_amdahl(ps, sp)
            at_ncore = one(scaling, algo=a, n=n, threads=ncore)
            fits[(a, n)] = {
                "f_kernel": fk,
                "f_program": fp,
                # Share of a 1-thread run spent in the parallel phase (by timing, not by fit).
                "f_phase": base["t_mul_mean"] / program_time(base),
                "s_ncore": base["t_mul_best"] / at_ncore["t_mul_best"] if at_ncore else float("nan"),
                "kf_ncore": karp_flatt(base["t_mul_best"] / at_ncore["t_mul_best"], ncore) if at_ncore else float("nan"),
                "s_best": base["t_mul_best"] / best["t_mul_best"],
                "p_best": best["threads"],
                "s_max_kernel": 1 / (1 - fk) if fk < 1 else float("inf"),
                "s_max_program": 1 / (1 - fp) if fp < 1 else float("inf"),
            }
    return fits


def write_summary(out, data, fits, ncore, ncpu):
    L = ["# Task 1 benchmark summary (auto-generated by plot.py)", ""]
    L.append("Numbers only - interpretation belongs in the report. Speedup is relative to the same "
             "algorithm at 1 thread. Amdahl fit: least squares on S(p) = 1/((1-f)+f/p) using "
             "2 <= p <= %d (physical cores); SMT points (p > %d, up to %d) are excluded from the fit." % (ncore, ncore, ncpu))
    L.append("")
    L.append("- **f_kernel**: fitted parallel fraction of the multiplication kernel alone.")
    L.append("- **f_program**: fitted parallel fraction of a whole run (serial init + 1 kernel + serial checksum).")
    L.append("- **f_phase**: measured share of a 1-thread run spent in the parallel phase (t_mul / total), "
             "i.e. the fraction that is *parallelisable by construction*.")
    L.append("- **e(%d)**: Karp-Flatt experimentally determined serial fraction at p=%d." % (ncore, ncore))
    L.append("- **S_max**: Amdahl upper bound 1/(1-f) for infinitely many processors.")
    L.append("")
    rows = []
    for (a, n), v in sorted(fits.items(), key=lambda kv: (ALGO_ORDER.index(kv[0][0]), kv[0][1])):
        rows.append([a, n, "%.4f" % v["f_kernel"], "%.4f" % v["f_program"], "%.4f" % v["f_phase"],
                     "%.2f" % v["s_ncore"], "%.3f" % v["kf_ncore"], "%.2f @ p=%d" % (v["s_best"], v["p_best"]),
                     "%.1f" % v["s_max_kernel"], "%.1f" % v["s_max_program"]])
    L.append(md_table(["algo", "n", "f_kernel", "f_program", "f_phase", "S(%d)" % ncore, "e(%d)" % ncore,
                       "best S", "S_max kernel", "S_max program"], rows))
    L.append("")

    scaling = data["scaling"]
    L.append("## Phase times at 1 thread (seconds)")
    L.append("")
    rows = []
    for a in ALGO_ORDER:
        for n in sorted({r["n"] for r in where(scaling, algo=a)}):
            r = one(scaling, algo=a, n=n, threads=1)
            if r:
                rows.append([a, n, "%.6f" % r["t_init"], "%.6f" % r["t_mul_mean"], "%.6f" % r["t_post"],
                             "%.2f%%" % (100 * (r["t_init"] + r["t_post"]) / program_time(r))])
    L.append(md_table(["algo", "n", "t_init (serial)", "t_mul (parallel)", "t_post (serial)", "serial share"], rows))
    L.append("")

    algos_rows = data["algos"]
    if algos_rows:
        L.append("## Algorithm comparison - GFLOP/s")
        L.append("")
        ts = sorted({r["threads"] for r in algos_rows})
        rows = []
        for n in sorted({r["n"] for r in algos_rows}):
            for a in ALGO_ORDER:
                vals = [one(algos_rows, algo=a, n=n, threads=t) for t in ts]
                if any(vals):
                    rows.append([n, a] + ["%.2f" % v["gflops"] if v else "-" for v in vals])
        L.append(md_table(["n", "algo"] + ["%d thr" % t for t in ts], rows))
        L.append("")

    for key, title, cols in (
        ("sched", "Schedule x chunk (ikj) - kernel time (s)", ("n", "threads", "schedule", "chunk")),
        ("block", "Block size (blocked) - GFLOP/s", ("n", "threads", "block")),
        ("flags", "Compiler flags - GFLOP/s", ("label", "algo", "n", "threads")),
        ("bind", "OMP_PROC_BIND / OMP_PLACES - kernel time (s)", ("label", "algo", "threads")),
        ("affinity", "taskset CPU mask - kernel time (s)", ("label", "threads")),
        ("wait", "OMP_WAIT_POLICY - kernel time (s)", ("label", "n", "threads")),
        ("nice", "nice + competing load - kernel time (s)", ("label", "threads")),
    ):
        rows_in = data[key]
        if not rows_in:
            continue
        L.append("## " + title)
        L.append("")
        rows = [[r[c] for c in cols] + ["%.6f" % r["t_mul_best"], "%.2f" % r["gflops"]] for r in rows_in]
        L.append(md_table(list(cols) + ["t_mul_best", "GFLOP/s"], rows))
        L.append("")

    mem = data["memory"]
    if mem:
        L.append("## Memory")
        L.append("")
        rows = [[r["algo"], r["n"], r["threads"], "%.2f" % r["mem_theory_mb"], "%.2f" % r["maxrss_mb"],
                 "%.2f" % (r["time_maxrss_kb"] / 1024), "%.1f" % r["vmpeak_mb"]] for r in mem]
        L.append(md_table(["algo", "n", "threads", "theory MiB", "peak RSS MiB (getrusage)",
                           "peak RSS MiB (/usr/bin/time)", "VmPeak MiB"], rows))
        L.append("")
    stack = data["stack"]
    if stack:
        L.append("## Per-thread memory (n = 100)")
        L.append("")
        rows = [[label_kv(r["label"])["stack"], r["threads"], "%.2f" % r["maxrss_mb"], "%.1f" % r["vmpeak_mb"]]
                for r in stack]
        L.append(md_table(["OMP_STACKSIZE", "threads", "peak RSS MiB", "VmPeak MiB"], rows))
        L.append("")

    bad = [r for rs in data.values() for r in rs if r.get("verified") == 0]
    L.append("Verification failures in benchmark data: %d" % len(bad))
    L.append("")
    path = os.path.join(out, "summary.md")
    with open(path, "w") as f:
        f.write("\n".join(L))
    print("  wrote", path)


# ------------------------------------------------------------------ main
def main():
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "results")
    raw = os.path.join(out, "raw")
    figdir = os.path.join(out, "figures")
    os.makedirs(figdir, exist_ok=True)

    names = ["scaling", "algos", "sched", "block", "flags", "bind", "affinity", "wait", "nice", "memory", "stack"]
    data = {k: load(raw, k + ".csv") for k in names}
    if not data["scaling"]:
        sys.exit("no scaling.csv found in %s - run bench.sh first" % raw)

    ncpu = os.cpu_count() or 1
    ncore = ncpu
    try:
        with open("/proc/cpuinfo") as f:
            ncore = len({line for line in f if line.startswith("core id")}) or ncpu
    except OSError:
        pass

    fits = amdahl_fits(data["scaling"], ncore)
    fig_algos(data["algos"], figdir)
    fig_scaling(data["scaling"], figdir, ncore)
    fig_sched(data["sched"], figdir)
    fig_block(data["block"], figdir)
    fig_flags(data["flags"], figdir)
    fig_bind(data["bind"], figdir)
    fig_affinity_nice(data["affinity"], data["nice"], figdir)
    fig_memory(data["memory"], data["stack"], figdir)
    fig_amdahl(data["scaling"], figdir, ncore, fits)
    write_summary(out, data, fits, ncore, ncpu)


if __name__ == "__main__":
    main()
