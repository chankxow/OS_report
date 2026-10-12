//! `--snapshot`: a one-shot text report of a process's memory, for terminals, files and reports.

use std::fmt::Write;

use crate::model::{fmt_bytes, layout_map, summarize, Kind, MapCell};
use crate::procfs::{MemInfo, Snapshot};

/// Wraps text in ANSI truecolor escapes when color is on.
struct Paint {
    color: bool,
}

impl Paint {
    fn fg(&self, text: &str, (r, g, b): (u8, u8, u8)) -> String {
        if self.color {
            format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn cell(&self, ch: char, bg: (u8, u8, u8)) -> String {
        if self.color {
            let (r, g, b) = bg;
            format!("\x1b[48;2;{r};{g};{b}m\x1b[38;2;20;20;30m{ch}\x1b[0m")
        } else {
            ch.to_string()
        }
    }

    fn bold(&self, text: &str) -> String {
        if self.color {
            format!("\x1b[1m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}

fn local_time() -> String {
    // SAFETY: time() with a null pointer only returns the time; localtime_r fills our zeroed tm.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

fn pct(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

const MAP_ROWS: usize = 3;

/// Renders the whole report. `width` is the number of map cells per line.
pub fn render(snap: &Snapshot, mem: &MemInfo, color: bool, width: usize) -> String {
    let p = Paint { color };
    let accent = (255, 184, 108);
    let dim = (120, 125, 140);
    let st = &snap.status;
    let mut out = String::new();

    let _ = writeln!(
        out,
        "{} PID {} ({})  {}",
        p.bold(&p.fg("mmap snapshot", accent)),
        snap.pid,
        st.name,
        p.fg(&local_time(), dim)
    );
    let _ = writeln!(out, "  command  {}", snap.cmdline);
    if let Some(exe) = &snap.exe {
        let _ = writeln!(out, "  exe      {exe}");
    }
    let _ = writeln!(out, "  state    {}   threads {}   uid {}", st.state, st.threads, st.uid);
    if let Some(e) = &snap.map_error {
        let _ = writeln!(out, "  {}", p.fg(&format!("warning: {e}"), (255, 85, 85)));
    }

    // Process totals straight from /proc/PID/status.
    let _ = writeln!(out, "\n{}", p.bold("Memory (from /proc/PID/status)"));
    let row = |out: &mut String, a: &str, ka: &str, b: &str, kb: &str| {
        let _ = writeln!(out, "  {a:<9}{:>9}    {b:<9}{:>9}", fmt_bytes(st.bytes(ka)), fmt_bytes(st.bytes(kb)));
    };
    row(&mut out, "VmSize", "VmSize", "VmPeak", "VmPeak");
    row(&mut out, "VmRSS", "VmRSS", "VmHWM", "VmHWM");
    row(&mut out, "RssAnon", "RssAnon", "RssFile", "RssFile");
    row(&mut out, "RssShmem", "RssShmem", "VmSwap", "VmSwap");
    row(&mut out, "VmData", "VmData", "VmStk", "VmStk");
    row(&mut out, "VmExe", "VmExe", "VmLib", "VmLib");
    let used = mem.total.saturating_sub(mem.available);
    let _ = writeln!(
        out,
        "  system   {} used of {} ({:.0}%), this process RSS = {:.2}% of RAM",
        fmt_bytes(used),
        fmt_bytes(mem.total),
        pct(used, mem.total),
        pct(st.bytes("VmRSS"), mem.total)
    );

    if snap.regions.is_empty() {
        if snap.map_error.is_none() {
            let _ = writeln!(out, "\n(no user-space mappings: this is probably a kernel thread)");
        }
        return out;
    }

    // Per-kind summary.
    let stats = summarize(&snap.regions);
    let total_rss: u64 = stats.iter().map(|s| s.rss).sum();
    let _ = writeln!(out, "\n{}", p.bold("Summary by region kind (from /proc/PID/smaps)"));
    let _ = writeln!(
        out,
        "  {:<11}{:>7}{:>9}{:>9}{:>9}{:>9}  {}",
        "KIND", "REGIONS", "VIRTUAL", "RSS", "PSS", "SWAP", "SHARE OF RSS"
    );
    for s in &stats {
        let share = pct(s.rss, total_rss);
        let bar: String = "█".repeat((share / 5.0).round() as usize);
        let _ = writeln!(
            out,
            "  {} {:<9}{:>7}{:>9}{:>9}{:>9}{:>9}  {} {share:.1}%",
            p.fg("■", s.kind.rgb()),
            s.kind.label(),
            s.count,
            fmt_bytes(s.size),
            fmt_bytes(s.rss),
            fmt_bytes(s.pss),
            fmt_bytes(s.swap),
            p.fg(&bar, s.kind.rgb())
        );
    }
    let sum = |f: fn(&crate::model::KindStat) -> u64| stats.iter().map(f).sum::<u64>();
    let _ = writeln!(
        out,
        "  {:<11}{:>7}{:>9}{:>9}{:>9}{:>9}",
        "total",
        snap.regions.len(),
        fmt_bytes(sum(|s| s.size)),
        fmt_bytes(sum(|s| s.rss)),
        fmt_bytes(sum(|s| s.pss)),
        fmt_bytes(sum(|s| s.swap))
    );
    if snap.has_smaps {
        let _ = writeln!(
            out,
            "  check: sum of per-region Rss = {} B, VmRSS = {} B",
            total_rss,
            st.bytes("VmRSS")
        );
    }

    // Address-space map: MAP_ROWS lines read left to right, top to bottom.
    let _ = writeln!(
        out,
        "\n{}",
        p.bold("Address space map (low -> high addresses; width ~ log2(size); gaps collapsed)")
    );
    let layout = layout_map(&snap.regions, width * MAP_ROWS);
    for line in layout.cells.chunks(width) {
        let first_addr = line.iter().find_map(|c| match c {
            MapCell::Region { idx, .. } => Some(snap.regions[*idx].start),
            _ => None,
        });
        let label = first_addr.map_or_else(|| " ".repeat(14), |a| format!("0x{a:012x}"));
        let mut s = format!("  {} ", p.fg(&label, dim));
        for cell in line {
            s += &match *cell {
                MapCell::Empty => " ".to_string(),
                MapCell::Gap => p.fg("·", dim),
                MapCell::Region { idx, resident } => {
                    let k = snap.regions[idx].kind;
                    if resident {
                        p.cell(k.letter(), k.rgb())
                    } else {
                        p.cell(k.letter().to_ascii_lowercase(), k.dim_rgb())
                    }
                }
            };
        }
        let _ = writeln!(out, "{s}");
    }
    let legend: Vec<String> = Kind::ALL
        .iter()
        .filter(|k| stats.iter().any(|s| s.kind == **k))
        .map(|k| format!("{}={}", p.fg(&k.letter().to_string(), k.rgb()), k.label()))
        .collect();
    let _ = writeln!(out, "  {}", legend.join("  "));
    let _ = writeln!(out, "  UPPER = resident share of the region, lower = mapped but not in RAM, · = unmapped gap");

    // Full region table.
    let _ = writeln!(out, "\n{}", p.bold("Regions (from /proc/PID/maps + smaps)"));
    let _ = writeln!(
        out,
        "  {:<16} {:<16} {:>7} {:<4} {:>7} {:>7} {:>7} {:>7} {:<9} {}",
        "START", "END", "SIZE", "PERM", "RSS", "PSS", "DIRTY", "SWAP", "KIND", "PATH"
    );
    for r in &snap.regions {
        let stat = |v: u64| if r.smaps.is_some() { fmt_bytes(v) } else { "-".to_string() };
        let _ = writeln!(
            out,
            "  {:<16x} {:<16x} {:>7} {:<4} {:>7} {:>7} {:>7} {:>7} {} {}",
            r.start,
            r.end,
            fmt_bytes(r.size()),
            r.perms,
            stat(r.rss()),
            stat(r.pss()),
            stat(r.dirty()),
            stat(r.swap()),
            p.fg(&format!("{:<9}", r.kind.label()), r.kind.rgb()),
            r.path
        );
    }
    out
}
