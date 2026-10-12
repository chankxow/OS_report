#!/usr/bin/env bash
# bench.sh - run every experiment for the parallel matrix multiplication report
#            and write the results as CSV files.
#
# Educational script for an Operating Systems course report.
#
# Usage:  ./bench.sh [OUT_DIR] [quick]
#   OUT_DIR  where results go (default: ./results). Binaries are built into
#            OUT_DIR/bin, so nothing is added next to the source code.
#   quick    tiny sizes / few repeats, to smoke-test the script in ~1 minute.
#
# DO NO HARM:
#   - needs no root and changes no system setting (CPU governor etc. are only
#     *recorded* in sysinfo.txt);
#   - matrix size is capped inside parallel.c (n <= 4096, ~400 MB);
#   - the background-load generator is limited by `timeout` and killed on exit.

set -euo pipefail

SRC_DIR="$(cd "$(dirname "$0")" && pwd)"
OUT="${1:-$SRC_DIR/results}"
MODE="${2:-full}"
RAW="$OUT/raw"
BIN="$OUT/bin"
mkdir -p "$RAW" "$BIN"

NCPU="$(nproc)"
NCORE="$(lscpu -p=CORE | grep -v '^#' | sort -u | wc -l)"

if [[ "$MODE" == "quick" ]]; then
    SIZES=(100 200)
    THREADS=(1 2 4)
    MID_N=200
    NAIVE_MAX=200
    reps() { echo 2; }
else
    SIZES=(100 200 400 800 1600 3200)
    THREADS=(1 2 3 4 5 6 7 8 10 12 14 16)
    MID_N=1600
    NAIVE_MAX=1600
    # More repeats for short runs (less noise), fewer for long runs.
    reps() {
        if   (( $1 <= 400 ));  then echo 10
        elif (( $1 <= 800 ));  then echo 5
        elif (( $1 <= 1600 )); then echo 3
        else                        echo 2
        fi
    }
fi

log() { printf '[%(%H:%M:%S)T] %s\n' -1 "$*" >&2; }

# ---------------------------------------------------------------- build
CFLAGS_BASE="-fopenmp -Wall -Wextra"
log "building binaries into $BIN"
gcc -O2 $CFLAGS_BASE -o "$BIN/parallel"        "$SRC_DIR/parallel.c"
gcc -O0 $CFLAGS_BASE -o "$BIN/parallel_O0"     "$SRC_DIR/parallel.c"
gcc -O3 -march=native $CFLAGS_BASE -o "$BIN/parallel_O3native" "$SRC_DIR/parallel.c"
P="$BIN/parallel"

header() { "$P" -H -n 2 -t 1 -r 1 | head -1; }

# Background CPU load (for the nice experiment). Started/stopped explicitly.
BG_PIDS=()
start_load() {
    local k
    for ((k = 0; k < NCPU; k++)); do
        timeout 600 sh -c 'while :; do :; done' &
        BG_PIDS+=("$!")
    done
    sleep 1
}
stop_load() {
    if ((${#BG_PIDS[@]})); then
        kill "${BG_PIDS[@]}" 2>/dev/null || true
        wait "${BG_PIDS[@]}" 2>/dev/null || true
    fi
    BG_PIDS=()
}
trap stop_load EXIT INT TERM

# ---------------------------------------------------------------- sysinfo
log "collecting system information"
{
    echo "date: $(date -Iseconds)"
    echo "uname: $(uname -a)"
    echo "gcc: $(gcc --version | head -1)"
    echo "logical CPUs: $NCPU   physical cores: $NCORE"
    echo "governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo n/a)"
    echo "energy_perf_preference: $(cat /sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference 2>/dev/null || echo n/a)"
    echo "boost: $(cat /sys/devices/system/cpu/cpufreq/boost 2>/dev/null || echo n/a)"
    echo "SMT control: $(cat /sys/devices/system/cpu/smt/control 2>/dev/null || echo n/a)"
    echo "cpu0 SMT siblings: $(cat /sys/devices/system/cpu/cpu0/topology/thread_siblings_list 2>/dev/null || echo n/a)"
    echo "transparent hugepages: $(cat /sys/kernel/mm/transparent_hugepage/enabled 2>/dev/null || echo n/a)"
    echo "ulimit -s (KiB): $(ulimit -s)"
    echo "loadavg at start: $(cat /proc/loadavg)"
    echo
    echo "---- lscpu ----"
    lscpu
    echo
    echo "---- free -h ----"
    free -h
} > "$RAW/sysinfo.txt"

# ---------------------------------------------------------------- 1. threads x n
# Main scaling data: used for time/speedup/efficiency graphs and Amdahl fit.
log "exp 1: thread scaling x matrix size"
F="$RAW/scaling.csv"; header > "$F"
for n in "${SIZES[@]}"; do
    for a in naive ikj blocked; do
        [[ "$a" == naive ]] && (( n > NAIVE_MAX )) && continue
        for t in "${THREADS[@]}"; do
            log "  $a n=$n t=$t"
            "$P" -l scaling -a "$a" -n "$n" -t "$t" -r "$(reps "$n")" >> "$F"
        done
    done
done

# ---------------------------------------------------------------- 2. algorithms
log "exp 2: algorithm comparison"
F="$RAW/algos.csv"; header > "$F"
for n in "${SIZES[@]}"; do
    for a in naive ikj transpose blocked; do
        [[ "$a" == naive ]] && (( n > NAIVE_MAX )) && continue
        for t in 1 "$NCORE" "$NCPU"; do
            "$P" -l algos -a "$a" -n "$n" -t "$t" -r "$(reps "$n")" >> "$F"
        done
    done
done

# ---------------------------------------------------------------- 3. program settings
log "exp 3a: OpenMP schedule x chunk"
F="$RAW/sched.csv"; header > "$F"
for n in 400 "$MID_N"; do
    for s in static dynamic guided; do
        for c in 0 1 4 16 64; do
            for t in "$NCORE" "$NCPU"; do
                "$P" -l sched -a ikj -n "$n" -t "$t" -s "$s" -c "$c" -r "$(reps "$n")" >> "$F"
            done
        done
    done
done

log "exp 3b: block size"
F="$RAW/block.csv"; header > "$F"
for b in 8 16 32 48 64 96 128 256 512; do
    for t in 1 "$NCPU"; do
        "$P" -l block -a blocked -b "$b" -n "$MID_N" -t "$t" -r "$(reps "$MID_N")" >> "$F"
    done
done

log "exp 3c: compiler optimisation flags"
F="$RAW/flags.csv"; header > "$F"
for v in O0 O2 O3native; do
    bin="$BIN/parallel_$v"; [[ "$v" == O2 ]] && bin="$P"
    for a in naive ikj blocked; do
        n="$MID_N"; [[ "$v" == O0 ]] && n=800       # -O0 is very slow
        for t in 1 "$NCPU"; do
            "$bin" -l "flags=$v" -a "$a" -n "$n" -t "$t" -r "$(reps "$n")" >> "$F"
        done
    done
done

# ---------------------------------------------------------------- 4. OS settings
log "exp 4a: thread placement (OMP_PROC_BIND x OMP_PLACES)"
F="$RAW/bind.csv"; header > "$F"
for bind in false close spread; do
    for places in threads cores; do
        [[ "$bind" == false && "$places" == cores ]] && continue  # places ignored when unbound
        for t in 4 "$NCORE" "$NCPU"; do
            for a in ikj blocked; do
                OMP_PROC_BIND="$bind" OMP_PLACES="$places" \
                    "$P" -l "bind=$bind;places=$places" -a "$a" -n "$MID_N" -t "$t" -r "$(reps "$MID_N")" >> "$F"
            done
        done
    done
done

log "exp 4b: CPU affinity mask (taskset) / oversubscription"
F="$RAW/affinity.csv"; header > "$F"
for mask in "0" "0-1" "0-3" "0-7" "0-$((NCPU - 1))"; do
    for t in 1 4 "$NCPU"; do
        taskset -c "$mask" "$P" -l "cpus=$mask" -a ikj -n "$MID_N" -t "$t" -r "$(reps "$MID_N")" >> "$F"
    done
done

log "exp 4c: OMP_WAIT_POLICY"
F="$RAW/wait.csv"; header > "$F"
for w in active passive; do
    for n in 100 400 "$MID_N"; do
        OMP_WAIT_POLICY="$w" "$P" -l "wait=$w" -a ikj -n "$n" -t "$NCPU" -r "$(reps "$n")" >> "$F"
    done
done

log "exp 4d: scheduler priority (nice) with and without competing load"
F="$RAW/nice.csv"; header > "$F"
for load in none busy; do
    [[ "$load" == busy ]] && start_load
    for nv in 0 19; do
        for t in 1 "$NCPU"; do
            nice -n "$nv" "$P" -l "load=$load;nice=$nv" -a ikj -n "$MID_N" -t "$t" -r "$(reps "$MID_N")" >> "$F"
        done
    done
    stop_load
done

# ---------------------------------------------------------------- 5. memory
log "exp 5a: memory vs n and threads (+ /usr/bin/time cross-check)"
F="$RAW/memory.csv"
echo "$(header),time_maxrss_kb" > "$F"
for n in "${SIZES[@]}"; do
    for a in ikj transpose; do
        for t in 1 "$NCPU"; do
            tf="$(mktemp)"
            row="$(/usr/bin/time -f '%M' -o "$tf" "$P" -l memory -a "$a" -n "$n" -t "$t" -r 1)"
            echo "$row,$(tail -1 "$tf")" >> "$F"
            rm -f "$tf"
        done
    done
done

log "exp 5b: per-thread memory overhead (OMP_STACKSIZE)"
F="$RAW/stack.csv"; header > "$F"
for ss in default 1M 64M; do
    for t in 1 2 4 8 "$NCPU"; do
        if [[ "$ss" == default ]]; then
            "$P" -l "stack=$ss" -a ikj -n 100 -t "$t" -r 1 >> "$F"
        else
            OMP_STACKSIZE="$ss" "$P" -l "stack=$ss" -a ikj -n 100 -t "$t" -r 1 >> "$F"
        fi
    done
done

echo "loadavg at end: $(cat /proc/loadavg)" >> "$RAW/sysinfo.txt"
log "done - results in $RAW"
