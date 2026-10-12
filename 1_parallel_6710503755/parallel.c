/*
 * parallel.c - Parallel dense matrix multiplication C = A x B (n x n, double)
 *              using OpenMP.
 *
 * Educational program written for an Operating Systems course report.
 * Not intended for production use.
 *
 * Build:  gcc -O2 -fopenmp -Wall -Wextra -o parallel parallel.c
 * Run:    ./parallel -n 800 -t 8 -a ikj -v
 *         ./parallel -h            (show all options)
 *
 * Output (-o): when stdout is a terminal the result is shown as readable
 * tables; when stdout is redirected or piped it is ONE CSV row per run (use -H
 * to also print the header), so a benchmark script can append it to a file.
 *
 * Phases (timed separately with omp_get_wtime, so serial vs parallel parts
 * can be analysed with Amdahl's law):
 *   t_init   : SERIAL   - malloc + deterministic fill of A and B
 *   t_mul    : PARALLEL - the multiplication kernel (best and mean of -r runs)
 *   t_post   : SERIAL   - checksum of C
 *   t_verify : SERIAL   - optional correctness check against a serial reference
 */

#include <errno.h>
#include <getopt.h>
#include <math.h>
#include <stdarg.h>
#include <omp.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <unistd.h>

#define MAX_N 4096          /* 3 * 4096^2 * 8 B = 384 MiB: safety cap (DO NO HARM) */
#define FULL_VERIFY_MAX_N 800
#define SAMPLE_VERIFY_COUNT 2000
#define VERIFY_TOL 1e-9

typedef enum { ALG_NAIVE, ALG_IKJ, ALG_TRANSPOSE, ALG_BLOCKED } algo_t;
typedef enum { OUT_AUTO, OUT_TABLE, OUT_CSV } output_t;

static const char *algo_names[] = { "naive", "ikj", "transpose", "blocked" };

typedef struct {
    int n;
    int threads;
    algo_t algo;
    int block;
    omp_sched_t sched;
    const char *sched_name;
    int chunk;              /* 0 = OpenMP default chunk */
    int repeats;
    int verify;
    int header;
    output_t output;
    const char *label;
} config_t;

/* ---------- helpers ---------- */

/* Small 64-bit LCG so every run gets exactly the same input matrices. */
static uint64_t lcg_next(uint64_t *state)
{
    *state = *state * 6364136223846793005ULL + 1442695040888963407ULL;
    return *state;
}

/* Uniform double in [0, 1). Non-negative values avoid cancellation, which
 * keeps the relative-error check meaningful. */
static double lcg_double(uint64_t *state)
{
    return (double)(lcg_next(state) >> 11) * (1.0 / 9007199254740992.0);
}

static double *alloc_matrix(int n)
{
    size_t bytes = (size_t)n * (size_t)n * sizeof(double);
    double *m = malloc(bytes);
    if (m == NULL) {
        fprintf(stderr, "error: cannot allocate %zu bytes\n", bytes);
        exit(EXIT_FAILURE);
    }
    return m;
}

static void fill_matrix(double *m, int n, uint64_t seed)
{
    uint64_t state = seed;
    size_t total = (size_t)n * (size_t)n;
    for (size_t i = 0; i < total; i++)
        m[i] = lcg_double(&state);
}

static double peak_rss_mb(void)
{
    struct rusage ru;
    if (getrusage(RUSAGE_SELF, &ru) != 0)
        return -1.0;
    return (double)ru.ru_maxrss / 1024.0;   /* Linux reports KiB */
}

/* Peak virtual memory size (VmPeak) from /proc/self/status, in MiB.
 * Unlike RSS this also counts reserved-but-untouched memory such as the
 * stack of every OpenMP worker thread. Returns -1 if unavailable. */
static double peak_vm_mb(void)
{
    FILE *f = fopen("/proc/self/status", "r");
    if (f == NULL)
        return -1.0;
    char line[256];
    double mb = -1.0;
    while (fgets(line, sizeof line, f) != NULL) {
        long kb;
        if (sscanf(line, "VmPeak: %ld kB", &kb) == 1) {
            mb = (double)kb / 1024.0;
            break;
        }
    }
    fclose(f);
    return mb;
}

/* ---------- multiplication kernels ----------
 * Every kernel parallelises the outermost loop over rows (or row blocks) of C.
 * Each C element is written by exactly one thread, so no locks or atomics are
 * needed. schedule(runtime) lets the -s/-c options choose the schedule. */

/* 1) Textbook i-j-k. Inner loop walks B down a column (stride n): poor cache use. */
static void mul_naive(const double *A, const double *B, double *C, int n)
{
    #pragma omp parallel for schedule(runtime)
    for (int i = 0; i < n; i++) {
        for (int j = 0; j < n; j++) {
            double sum = 0.0;
            for (int k = 0; k < n; k++)
                sum += A[(size_t)i * n + k] * B[(size_t)k * n + j];
            C[(size_t)i * n + j] = sum;
        }
    }
}

/* 2) Loop interchange i-k-j. Inner loop streams along rows of B and C
 *    (stride 1), which is cache friendly and vectorisable. */
static void mul_ikj(const double *A, const double *B, double *C, int n)
{
    #pragma omp parallel for schedule(runtime)
    for (int i = 0; i < n; i++) {
        double *c_row = &C[(size_t)i * n];
        for (int j = 0; j < n; j++)
            c_row[j] = 0.0;
        for (int k = 0; k < n; k++) {
            double a = A[(size_t)i * n + k];
            const double *b_row = &B[(size_t)k * n];
            for (int j = 0; j < n; j++)
                c_row[j] += a * b_row[j];
        }
    }
}

/* 3) Transpose B first (BT[j][k] = B[k][j]), then each C[i][j] is a dot
 *    product of two contiguous rows. Costs n^2 extra memory. */
static void mul_transpose(const double *A, const double *B, double *BT,
                          double *C, int n)
{
    #pragma omp parallel for schedule(runtime)
    for (int k = 0; k < n; k++)
        for (int j = 0; j < n; j++)
            BT[(size_t)j * n + k] = B[(size_t)k * n + j];

    #pragma omp parallel for schedule(runtime)
    for (int i = 0; i < n; i++) {
        const double *a_row = &A[(size_t)i * n];
        for (int j = 0; j < n; j++) {
            const double *bt_row = &BT[(size_t)j * n];
            double sum = 0.0;
            for (int k = 0; k < n; k++)
                sum += a_row[k] * bt_row[k];
            C[(size_t)i * n + j] = sum;
        }
    }
}

/* 4) Blocked (tiled) i-k-j. Works on bs x bs tiles so the active parts of
 *    A, B and C stay in cache. Parallel over row blocks of C. */
static void mul_blocked(const double *A, const double *B, double *C, int n, int bs)
{
    int nblocks = (n + bs - 1) / bs;

    #pragma omp parallel for schedule(runtime)
    for (int ib = 0; ib < nblocks; ib++) {
        int i0 = ib * bs;
        int i1 = (i0 + bs < n) ? i0 + bs : n;

        for (int i = i0; i < i1; i++)
            memset(&C[(size_t)i * n], 0, (size_t)n * sizeof(double));

        for (int k0 = 0; k0 < n; k0 += bs) {
            int k1 = (k0 + bs < n) ? k0 + bs : n;
            for (int j0 = 0; j0 < n; j0 += bs) {
                int j1 = (j0 + bs < n) ? j0 + bs : n;
                for (int i = i0; i < i1; i++) {
                    double *c_row = &C[(size_t)i * n];
                    for (int k = k0; k < k1; k++) {
                        double a = A[(size_t)i * n + k];
                        const double *b_row = &B[(size_t)k * n];
                        for (int j = j0; j < j1; j++)
                            c_row[j] += a * b_row[j];
                    }
                }
            }
        }
    }
}

static void multiply(const config_t *cfg, const double *A, const double *B,
                     double *BT, double *C)
{
    switch (cfg->algo) {
    case ALG_NAIVE:     mul_naive(A, B, C, cfg->n); break;
    case ALG_IKJ:       mul_ikj(A, B, C, cfg->n); break;
    case ALG_TRANSPOSE: mul_transpose(A, B, BT, C, cfg->n); break;
    case ALG_BLOCKED:   mul_blocked(A, B, C, cfg->n, cfg->block); break;
    }
}

/* ---------- verification (serial) ---------- */

static double rel_err(double got, double ref)
{
    double denom = fabs(ref) > 1e-300 ? fabs(ref) : 1e-300;
    return fabs(got - ref) / denom;
}

/* Returns the maximum relative error found. */
static double verify(const double *A, const double *B, const double *C, int n)
{
    double max_err = 0.0;

    if (n <= FULL_VERIFY_MAX_N) {
        /* Full serial reference with the plain i-j-k formula. */
        for (int i = 0; i < n; i++) {
            for (int j = 0; j < n; j++) {
                double sum = 0.0;
                for (int k = 0; k < n; k++)
                    sum += A[(size_t)i * n + k] * B[(size_t)k * n + j];
                double e = rel_err(C[(size_t)i * n + j], sum);
                if (e > max_err)
                    max_err = e;
            }
        }
    } else {
        /* Large n: recompute a random sample of entries. */
        uint64_t state = 12345;
        for (int s = 0; s < SAMPLE_VERIFY_COUNT; s++) {
            int i = (int)(lcg_next(&state) % (uint64_t)n);
            int j = (int)(lcg_next(&state) % (uint64_t)n);
            double sum = 0.0;
            for (int k = 0; k < n; k++)
                sum += A[(size_t)i * n + k] * B[(size_t)k * n + j];
            double e = rel_err(C[(size_t)i * n + j], sum);
            if (e > max_err)
                max_err = e;
        }
    }
    return max_err;
}

/* ---------- output ---------- */

typedef struct {
    int team;
    double t_init, t_best, t_mean, t_post, t_verify, t_total;
    double gflops, mem_theory_mb, maxrss_mb, vmpeak_mb;
    double checksum, max_err;
    int verified;
} result_t;

static void print_csv(const config_t *cfg, const result_t *r)
{
    if (cfg->header)
        printf("label,algo,n,threads,schedule,chunk,block,t_init,t_mul_best,"
               "t_mul_mean,t_post,t_verify,t_total,gflops,mem_theory_mb,"
               "maxrss_mb,vmpeak_mb,checksum,max_rel_err,verified\n");
    printf("%s,%s,%d,%d,%s,%d,%d,%.6f,%.6f,%.6f,%.6f,%.6f,%.6f,%.3f,%.3f,%.3f,%.3f,%.10e,%.3e,%d\n",
           cfg->label, algo_names[cfg->algo], cfg->n, r->team, cfg->sched_name, cfg->chunk,
           cfg->algo == ALG_BLOCKED ? cfg->block : 0,
           r->t_init, r->t_best, r->t_mean, r->t_post, r->t_verify, r->t_total, r->gflops,
           r->mem_theory_mb, r->maxrss_mb, r->vmpeak_mb, r->checksum, r->max_err, r->verified);
}

/* A tiny box-drawing table: cells are plain ASCII strings, column widths are
 * computed from the contents, and a rule can be drawn above any row. */
#define TBL_COLS 4
#define TBL_ROWS 12
#define TBL_CELL 64

typedef struct {
    int ncols, nrows;
    const char *align;                      /* 'l' or 'r' for each column */
    int rule_above[TBL_ROWS];
    char cell[TBL_ROWS][TBL_COLS][TBL_CELL];
} table_t;

/* Adds one row of t->ncols strings; rule != 0 draws a line above it. */
static void tbl_add(table_t *t, int rule, ...)
{
    va_list ap;
    va_start(ap, rule);
    for (int c = 0; c < t->ncols; c++)
        snprintf(t->cell[t->nrows][c], TBL_CELL, "%s", va_arg(ap, const char *));
    va_end(ap);
    t->rule_above[t->nrows++] = rule;
}

static void tbl_rule(const table_t *t, const int *w,
                     const char *left, const char *mid, const char *right)
{
    fputs(left, stdout);
    for (int c = 0; c < t->ncols; c++) {
        for (int i = 0; i < w[c] + 2; i++)
            fputs("─", stdout);
        fputs(c + 1 < t->ncols ? mid : right, stdout);
    }
    putchar('\n');
}

static void tbl_print(const char *title, const table_t *t)
{
    int w[TBL_COLS] = { 0 };
    for (int r = 0; r < t->nrows; r++)
        for (int c = 0; c < t->ncols; c++) {
            int len = (int)strlen(t->cell[r][c]);
            if (len > w[c])
                w[c] = len;
        }

    printf("\n %s\n", title);
    tbl_rule(t, w, "┌", "┬", "┐");
    for (int r = 0; r < t->nrows; r++) {
        if (r > 0 && t->rule_above[r])
            tbl_rule(t, w, "├", "┼", "┤");
        fputs("│", stdout);
        for (int c = 0; c < t->ncols; c++)
            printf(t->align[c] == 'r' ? " %*s │" : " %-*s │", w[c], t->cell[r][c]);
        putchar('\n');
    }
    tbl_rule(t, w, "└", "┴", "┘");
}

/* snprintf into one of a few rotating buffers, so several results can be
 * passed to tbl_add() in the same call. */
static const char *fmt(const char *format, ...)
{
    static char buf[16][TBL_CELL];
    static int next = 0;
    char *b = buf[next];
    next = (next + 1) % 16;
    va_list ap;
    va_start(ap, format);
    vsnprintf(b, TBL_CELL, format, ap);
    va_end(ap);
    return b;
}

static void print_table(const config_t *cfg, const result_t *r)
{
    table_t t;

    printf("\n Parallel matrix multiplication  C = A x B  (OpenMP)\n");

    memset(&t, 0, sizeof t);
    t.ncols = 2;
    t.align = "ll";
    tbl_add(&t, 0, "Label", cfg->label);
    tbl_add(&t, 0, "Algorithm", algo_names[cfg->algo]);
    tbl_add(&t, 0, "Matrix size", fmt("%d x %d", cfg->n, cfg->n));
    tbl_add(&t, 0, "Threads", r->team == cfg->threads
                              ? fmt("%d", r->team)
                              : fmt("%d (requested %d)", r->team, cfg->threads));
    tbl_add(&t, 0, "Schedule", cfg->chunk == 0
                               ? fmt("%s (default chunk)", cfg->sched_name)
                               : fmt("%s, chunk %d", cfg->sched_name, cfg->chunk));
    tbl_add(&t, 0, "Block size", cfg->algo == ALG_BLOCKED ? fmt("%d", cfg->block) : "-");
    tbl_add(&t, 0, "Kernel repeats", fmt("%d", cfg->repeats));
    tbl_print("Configuration", &t);

    /* Share of the total uses the best kernel time, like t_total does. */
    double tot = r->t_total > 0.0 ? r->t_total : 1.0;
    memset(&t, 0, sizeof t);
    t.ncols = 4;
    t.align = "llrr";
    tbl_add(&t, 0, "Phase", "Type", "Time (s)", "Share");
    tbl_add(&t, 1, "init (malloc + fill)", "serial",
            fmt("%.6f", r->t_init), fmt("%.1f %%", 100.0 * r->t_init / tot));
    tbl_add(&t, 0, "multiply (best)", "parallel",
            fmt("%.6f", r->t_best), fmt("%.1f %%", 100.0 * r->t_best / tot));
    tbl_add(&t, 0, "multiply (mean)", "parallel", fmt("%.6f", r->t_mean), "-");
    tbl_add(&t, 0, "checksum", "serial",
            fmt("%.6f", r->t_post), fmt("%.1f %%", 100.0 * r->t_post / tot));
    if (cfg->verify)
        tbl_add(&t, 0, "verify", "serial",
                fmt("%.6f", r->t_verify), fmt("%.1f %%", 100.0 * r->t_verify / tot));
    tbl_add(&t, 1, "total", "", fmt("%.6f", r->t_total), "100.0 %");
    tbl_print("Timing", &t);

    memset(&t, 0, sizeof t);
    t.ncols = 2;
    t.align = "lr";
    tbl_add(&t, 0, "Performance", fmt("%.3f GFLOP/s", r->gflops));
    tbl_add(&t, 1, "Matrices (theory)", fmt("%.3f MiB", r->mem_theory_mb));
    tbl_add(&t, 0, "Peak RSS", fmt("%.3f MiB", r->maxrss_mb));
    tbl_add(&t, 0, "Peak virtual (VmPeak)", fmt("%.3f MiB", r->vmpeak_mb));
    tbl_add(&t, 1, "Checksum", fmt("%.10e", r->checksum));
    if (r->verified == -1) {
        tbl_add(&t, 0, "Verified", "not checked (use -v)");
    } else {
        tbl_add(&t, 0, "Max relative error", fmt("%.3e", r->max_err));
        tbl_add(&t, 0, "Verified", r->verified ? fmt("PASS (<= %.0e)", VERIFY_TOL)
                                               : fmt("FAIL (> %.0e)", VERIFY_TOL));
    }
    tbl_print("Results", &t);
    putchar('\n');
}

/* ---------- command line ---------- */

static void usage(const char *prog)
{
    fprintf(stderr,
        "Usage: %s [options]\n"
        "  -n N        matrix size (1..%d, default 800)\n"
        "  -t T        number of OpenMP threads (default: OMP_NUM_THREADS or all CPUs)\n"
        "  -a ALGO     naive | ikj | transpose | blocked (default ikj)\n"
        "  -b BS       block size for 'blocked' (default 64)\n"
        "  -s SCHED    static | dynamic | guided (default static)\n"
        "  -c CHUNK    schedule chunk size, 0 = OpenMP default (default 0)\n"
        "  -r R        repeat the kernel R times, report best and mean (default 3)\n"
        "  -v          verify result against a serial reference\n"
        "  -l LABEL    free-text label written in the first CSV column\n"
        "  -o FORMAT   table | csv (default: table on a terminal, csv otherwise)\n"
        "  -H          print the CSV header line first (csv output only)\n"
        "  -h          show this help\n",
        prog, MAX_N);
}

static int parse_int(const char *s, const char *name, int lo, int hi)
{
    char *end;
    errno = 0;
    long v = strtol(s, &end, 10);
    if (errno != 0 || *end != '\0' || v < lo || v > hi) {
        fprintf(stderr, "error: %s must be an integer in [%d, %d] (got '%s')\n",
                name, lo, hi, s);
        exit(EXIT_FAILURE);
    }
    return (int)v;
}

static void parse_args(int argc, char **argv, config_t *cfg)
{
    long ncpu = sysconf(_SC_NPROCESSORS_ONLN);
    int max_threads = (ncpu > 0 ? (int)ncpu : 1) * 4;   /* safety cap */

    cfg->n = 800;
    cfg->threads = omp_get_max_threads();
    cfg->algo = ALG_IKJ;
    cfg->block = 64;
    cfg->sched = omp_sched_static;
    cfg->sched_name = "static";
    cfg->chunk = 0;
    cfg->repeats = 3;
    cfg->verify = 0;
    cfg->header = 0;
    cfg->output = OUT_AUTO;
    cfg->label = "-";

    int opt;
    while ((opt = getopt(argc, argv, "n:t:a:b:s:c:r:l:o:vHh")) != -1) {
        switch (opt) {
        case 'n': cfg->n = parse_int(optarg, "-n", 1, MAX_N); break;
        case 't': cfg->threads = parse_int(optarg, "-t", 1, max_threads); break;
        case 'b': cfg->block = parse_int(optarg, "-b", 1, MAX_N); break;
        case 'c': cfg->chunk = parse_int(optarg, "-c", 0, MAX_N); break;
        case 'r': cfg->repeats = parse_int(optarg, "-r", 1, 100); break;
        case 'l': cfg->label = optarg; break;
        case 'v': cfg->verify = 1; break;
        case 'H': cfg->header = 1; break;
        case 'a':
            if      (strcmp(optarg, "naive") == 0)     cfg->algo = ALG_NAIVE;
            else if (strcmp(optarg, "ikj") == 0)       cfg->algo = ALG_IKJ;
            else if (strcmp(optarg, "transpose") == 0) cfg->algo = ALG_TRANSPOSE;
            else if (strcmp(optarg, "blocked") == 0)   cfg->algo = ALG_BLOCKED;
            else { fprintf(stderr, "error: unknown algorithm '%s'\n", optarg); exit(EXIT_FAILURE); }
            break;
        case 'o':
            if      (strcmp(optarg, "table") == 0) cfg->output = OUT_TABLE;
            else if (strcmp(optarg, "csv") == 0)   cfg->output = OUT_CSV;
            else { fprintf(stderr, "error: unknown output format '%s'\n", optarg); exit(EXIT_FAILURE); }
            break;
        case 's':
            if      (strcmp(optarg, "static") == 0)  cfg->sched = omp_sched_static;
            else if (strcmp(optarg, "dynamic") == 0) cfg->sched = omp_sched_dynamic;
            else if (strcmp(optarg, "guided") == 0)  cfg->sched = omp_sched_guided;
            else { fprintf(stderr, "error: unknown schedule '%s'\n", optarg); exit(EXIT_FAILURE); }
            cfg->sched_name = optarg;
            break;
        case 'h': usage(argv[0]); exit(EXIT_SUCCESS);
        default:  usage(argv[0]); exit(EXIT_FAILURE);
        }
    }
    if (optind < argc) {
        usage(argv[0]);
        exit(EXIT_FAILURE);
    }
    if (cfg->output == OUT_AUTO)
        cfg->output = isatty(STDOUT_FILENO) ? OUT_TABLE : OUT_CSV;
}

/* ---------- main ---------- */

int main(int argc, char **argv)
{
    config_t cfg;
    parse_args(argc, argv, &cfg);

    int n = cfg.n;
    omp_set_num_threads(cfg.threads);
    omp_set_schedule(cfg.sched, cfg.chunk);

    /* Actual team size (the OS / OpenMP runtime may give fewer threads). */
    int team = 0;
    #pragma omp parallel
    {
        #pragma omp single
        team = omp_get_num_threads();
    }

    /* Phase 1 (serial): allocate and initialise. */
    double t0 = omp_get_wtime();
    double *A = alloc_matrix(n);
    double *B = alloc_matrix(n);
    double *C = alloc_matrix(n);
    double *BT = (cfg.algo == ALG_TRANSPOSE) ? alloc_matrix(n) : NULL;
    fill_matrix(A, n, 1);
    fill_matrix(B, n, 2);
    double t_init = omp_get_wtime() - t0;

    /* Phase 2 (parallel): the kernel, repeated. */
    double t_best = INFINITY, t_sum = 0.0;
    for (int r = 0; r < cfg.repeats; r++) {
        double s = omp_get_wtime();
        multiply(&cfg, A, B, BT, C);
        double dt = omp_get_wtime() - s;
        t_sum += dt;
        if (dt < t_best)
            t_best = dt;
    }
    double t_mean = t_sum / cfg.repeats;

    /* Phase 3 (serial): checksum. */
    t0 = omp_get_wtime();
    double checksum = 0.0;
    for (size_t i = 0; i < (size_t)n * (size_t)n; i++)
        checksum += C[i];
    double t_post = omp_get_wtime() - t0;

    /* Phase 4 (serial, optional): verification. */
    int verified = -1;                  /* -1 = not checked */
    double max_err = NAN, t_verify = 0.0;
    if (cfg.verify) {
        t0 = omp_get_wtime();
        max_err = verify(A, B, C, n);
        t_verify = omp_get_wtime() - t0;
        verified = (max_err <= VERIFY_TOL) ? 1 : 0;
    }

    int nmat = (cfg.algo == ALG_TRANSPOSE) ? 4 : 3;
    double mem_theory_mb = (double)nmat * n * n * sizeof(double) / (1024.0 * 1024.0);
    double gflops = 2.0 * n * n * (double)n / t_best / 1e9;
    double t_total = t_init + t_best + t_post + t_verify;

    result_t res = {
        .team = team,
        .t_init = t_init, .t_best = t_best, .t_mean = t_mean,
        .t_post = t_post, .t_verify = t_verify, .t_total = t_total,
        .gflops = gflops, .mem_theory_mb = mem_theory_mb,
        .maxrss_mb = peak_rss_mb(), .vmpeak_mb = peak_vm_mb(),
        .checksum = checksum, .max_err = max_err, .verified = verified,
    };
    if (cfg.output == OUT_TABLE)
        print_table(&cfg, &res);
    else
        print_csv(&cfg, &res);

    free(A);
    free(B);
    free(C);
    free(BT);

    if (verified == 0) {
        fprintf(stderr, "error: verification FAILED (max relative error %.3e)\n", max_err);
        return EXIT_FAILURE;
    }
    return EXIT_SUCCESS;
}
