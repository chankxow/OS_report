//! Data model shared by the TUI and the text snapshot: memory regions, their
//! classification, per-kind summaries, RSS history and the address-space map layout.

use std::collections::VecDeque;

use ratatui::style::Color;

/// What a mapping is used for, derived from its path, permissions and neighbours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Exe,
    Heap,
    Stack,
    ThreadStack,
    Lib,
    File,
    Shm,
    Anon,
    Guard,
    Kernel,
}

impl Kind {
    pub const ALL: [Kind; 10] = [
        Kind::Exe,
        Kind::Heap,
        Kind::Stack,
        Kind::ThreadStack,
        Kind::Lib,
        Kind::File,
        Kind::Shm,
        Kind::Anon,
        Kind::Guard,
        Kind::Kernel,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Exe => "exe",
            Kind::Heap => "heap",
            Kind::Stack => "stack",
            Kind::ThreadStack => "thr-stack",
            Kind::Lib => "lib",
            Kind::File => "file",
            Kind::Shm => "shm",
            Kind::Anon => "anon",
            Kind::Guard => "guard",
            Kind::Kernel => "kernel",
        }
    }

    /// One-letter code used by the plain-text map.
    pub fn letter(self) -> char {
        match self {
            Kind::Exe => 'E',
            Kind::Heap => 'H',
            Kind::Stack => 'S',
            Kind::ThreadStack => 'T',
            Kind::Lib => 'L',
            Kind::File => 'F',
            Kind::Shm => 'M',
            Kind::Anon => 'A',
            Kind::Guard => 'G',
            Kind::Kernel => 'K',
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Kind::Exe => "the program's own executable (text, data, bss)",
            Kind::Heap => "[heap]: malloc arena grown with brk()",
            Kind::Stack => "[stack]: main thread stack, grows down on demand",
            Kind::ThreadStack => "anonymous stack of another thread (follows a guard page)",
            Kind::Lib => "shared library (.so) mapped from a file",
            Kind::File => "other file mapped with mmap()",
            Kind::Shm => "shared memory (/dev/shm, memfd, SysV, MAP_SHARED anon)",
            Kind::Anon => "anonymous private memory (large malloc, mmap, JIT...)",
            Kind::Guard => "no-access page(s): stack guard or reserved address space",
            Kind::Kernel => "kernel-provided page ([vdso], [vvar], [vsyscall])",
        }
    }

    /// Dracula-like palette, close to btop's default look.
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Kind::Exe => (255, 121, 198),
            Kind::Heap => (80, 250, 123),
            Kind::Stack => (255, 184, 108),
            Kind::ThreadStack => (241, 250, 140),
            Kind::Lib => (139, 233, 253),
            Kind::File => (189, 147, 249),
            Kind::Shm => (100, 149, 237),
            Kind::Anon => (255, 110, 110),
            Kind::Guard => (98, 114, 164),
            Kind::Kernel => (200, 200, 210),
        }
    }

    /// Same hue, darker: used for mapped-but-not-resident cells.
    pub fn dim_rgb(self) -> (u8, u8, u8) {
        let (r, g, b) = self.rgb();
        let f = |c: u8| (c as f32 * 0.38) as u8;
        (f(r), f(g), f(b))
    }

    pub fn color(self) -> Color {
        let (r, g, b) = self.rgb();
        Color::Rgb(r, g, b)
    }

    pub fn dim_color(self) -> Color {
        let (r, g, b) = self.dim_rgb();
        Color::Rgb(r, g, b)
    }
}

/// Per-region counters from /proc/PID/smaps. Numeric fields are in KiB as the kernel reports them.
#[derive(Clone, Debug, Default)]
pub struct Smaps {
    pub size: u64,
    pub rss: u64,
    pub pss: u64,
    pub shared_clean: u64,
    pub shared_dirty: u64,
    pub private_clean: u64,
    pub private_dirty: u64,
    pub anonymous: u64,
    pub swap: u64,
    pub locked: u64,
    /// Every "Key: value" line in file order, shown in the details popup.
    pub raw: Vec<(String, String)>,
}

impl Smaps {
    pub fn set(&mut self, key: &str, value: &str) {
        if let Some(kb) = value.strip_suffix(" kB").and_then(|v| v.trim().parse::<u64>().ok()) {
            match key {
                "Size" => self.size = kb,
                "Rss" => self.rss = kb,
                "Pss" => self.pss = kb,
                "Shared_Clean" => self.shared_clean = kb,
                "Shared_Dirty" => self.shared_dirty = kb,
                "Private_Clean" => self.private_clean = kb,
                "Private_Dirty" => self.private_dirty = kb,
                "Anonymous" => self.anonymous = kb,
                "Swap" => self.swap = kb,
                "Locked" => self.locked = kb,
                _ => {}
            }
        }
        self.raw.push((key.to_string(), value.to_string()));
    }
}

/// One line of /proc/PID/maps, plus its smaps counters when they could be read.
#[derive(Clone, Debug)]
pub struct Region {
    pub start: u64,
    pub end: u64,
    pub perms: String,
    pub offset: u64,
    pub dev: String,
    pub inode: u64,
    pub path: String,
    pub kind: Kind,
    pub smaps: Option<Smaps>,
}

impl Region {
    pub fn size(&self) -> u64 {
        self.end - self.start
    }

    fn kib(&self, field: impl Fn(&Smaps) -> u64) -> u64 {
        self.smaps.as_ref().map_or(0, |s| field(s) * 1024)
    }

    pub fn rss(&self) -> u64 {
        self.kib(|s| s.rss)
    }

    pub fn pss(&self) -> u64 {
        self.kib(|s| s.pss)
    }

    pub fn swap(&self) -> u64 {
        self.kib(|s| s.swap)
    }

    pub fn dirty(&self) -> u64 {
        self.kib(|s| s.private_dirty + s.shared_dirty)
    }

    pub fn resident_fraction(&self) -> f64 {
        if self.size() == 0 {
            0.0
        } else {
            (self.rss() as f64 / self.size() as f64).min(1.0)
        }
    }

    /// Short name for labels: file name for file mappings, the bracket tag or kind otherwise.
    pub fn short_name(&self) -> String {
        if self.path.is_empty() {
            self.kind.label().to_string()
        } else if self.path.starts_with('[') {
            self.path.trim_matches(|c| c == '[' || c == ']').to_string()
        } else {
            self.path.rsplit('/').next().unwrap_or(&self.path).to_string()
        }
    }
}

fn is_shared_lib(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.ends_with(".so") || name.contains(".so.")
}

/// Largest no-access mapping still treated as a thread-stack guard (glibc uses one page by default).
const MAX_GUARD: u64 = 64 * 1024;
/// Smallest thread stack (PTHREAD_STACK_MIN). Smaller guarded mappings are e.g. Rust's 8 KiB
/// alternate signal stacks, which stay "anon".
const MIN_THREAD_STACK: u64 = 16 * 1024;

/// Assigns a `Kind` to every region. `regions` must be in address order (as the kernel lists them).
pub fn classify(regions: &mut [Region], exe: Option<&str>) {
    let exe = exe
        .map(|e| e.trim_end_matches(" (deleted)").to_string())
        // /proc/PID/exe is unreadable for other users' processes; the first mapping is the executable.
        .or_else(|| regions.first().filter(|r| r.inode != 0).map(|r| r.path.clone()));
    for i in 0..regions.len() {
        let after_guard = i > 0 && {
            let prev = &regions[i - 1];
            prev.end == regions[i].start
                && prev.path.is_empty()
                && prev.perms.starts_with("---")
                && prev.size() <= MAX_GUARD
        };
        let r = &regions[i];
        let path = r.path.trim_end_matches(" (deleted)");
        let shared = r.perms.ends_with('s');
        let kind = if path == "[heap]" {
            Kind::Heap
        } else if path.starts_with("[stack") {
            Kind::Stack
        } else if matches!(path, "[vdso]" | "[vvar]" | "[vvar_vclock]" | "[vsyscall]" | "[uprobes]") {
            Kind::Kernel
        } else if path.starts_with('[') {
            Kind::Anon // named anonymous memory such as [anon:...]
        } else if path.starts_with("/dev/shm/")
            || path.starts_with("/memfd:")
            || path.starts_with("/SYSV")
            || (shared && (path.is_empty() || path == "/dev/zero"))
        {
            Kind::Shm
        } else if !path.is_empty() && exe.as_deref() == Some(path) {
            Kind::Exe
        } else if is_shared_lib(path) {
            Kind::Lib
        } else if !path.is_empty() {
            Kind::File
        } else if r.perms.starts_with("---") {
            Kind::Guard
        } else if after_guard && r.size() >= MIN_THREAD_STACK {
            Kind::ThreadStack
        } else {
            Kind::Anon
        };
        regions[i].kind = kind;
    }
}

/// Totals for one region kind.
#[derive(Clone, Debug)]
pub struct KindStat {
    pub kind: Kind,
    pub count: usize,
    pub size: u64,
    pub rss: u64,
    pub pss: u64,
    pub swap: u64,
    pub dirty: u64,
}

/// Per-kind totals for the kinds present, in `Kind::ALL` order.
pub fn summarize(regions: &[Region]) -> Vec<KindStat> {
    Kind::ALL
        .iter()
        .filter_map(|&kind| {
            let mut st = KindStat { kind, count: 0, size: 0, rss: 0, pss: 0, swap: 0, dirty: 0 };
            for r in regions.iter().filter(|r| r.kind == kind) {
                st.count += 1;
                st.size += r.size();
                st.rss += r.rss();
                st.pss += r.pss();
                st.swap += r.swap();
                st.dirty += r.dirty();
            }
            (st.count > 0).then_some(st)
        })
        .collect()
}

/// One point of the RSS history chart, values in MiB.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub t: f64,
    pub rss: f64,
    pub anon: f64,
    pub file: f64,
}

pub struct History {
    pub samples: VecDeque<Sample>,
    cap: usize,
}

impl History {
    pub fn new(cap: usize) -> Self {
        History { samples: VecDeque::with_capacity(cap), cap }
    }

    pub fn push(&mut self, s: Sample) {
        if self.samples.len() == self.cap {
            self.samples.pop_front();
        }
        self.samples.push_back(s);
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

/// One character cell of the address-space map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MapCell {
    Empty,
    /// Unmapped addresses between two regions.
    Gap,
    Region { idx: usize, resident: bool },
}

pub struct MapLayout {
    pub cells: Vec<MapCell>,
    /// For every region, the cell that represents it (its first cell, or where it would fall
    /// when it was too small to win a cell of its own).
    pub anchor: Vec<usize>,
}

/// Width given to an unmapped gap, relative to a 4 KiB region (weight 1).
const GAP_WEIGHT: f64 = 0.5;

/// The address space is sparse (a few MiB used across 128 TiB), so a linear scale would show
/// nothing. Each region gets a width proportional to log2(pages) + 1 and every gap a fixed width.
fn weight(r: &Region) -> f64 {
    (r.size().max(4096) as f64 / 4096.0).log2() + 1.0
}

pub fn layout_map(regions: &[Region], ncells: usize) -> MapLayout {
    let mut cells = vec![MapCell::Empty; ncells];
    let mut anchor = vec![0usize; regions.len()];
    if ncells == 0 || regions.is_empty() {
        return MapLayout { cells, anchor };
    }

    let mut items: Vec<(Option<usize>, f64)> = Vec::with_capacity(regions.len() * 2);
    for (i, r) in regions.iter().enumerate() {
        if i > 0 && regions[i - 1].end != r.start {
            items.push((None, GAP_WEIGHT));
        }
        items.push((Some(i), weight(r)));
    }
    let total: f64 = items.iter().map(|it| it.1).sum();
    let scale = ncells as f64 / total;
    let mut spans = Vec::with_capacity(items.len());
    let mut acc = 0.0;
    for it in &items {
        let lo = acc * scale;
        acc += it.1;
        spans.push((lo, acc * scale));
    }

    // Each cell shows the item that covers most of it.
    let mut k = 0;
    for (c, cell) in cells.iter_mut().enumerate() {
        let (lo, hi) = (c as f64, c as f64 + 1.0);
        while k < items.len() && spans[k].1 <= lo {
            k += 1;
        }
        let mut best: Option<(usize, f64)> = None;
        let mut j = k;
        while j < items.len() && spans[j].0 < hi {
            let overlap = spans[j].1.min(hi) - spans[j].0.max(lo);
            if best.map_or(true, |(_, b)| overlap > b) {
                best = Some((j, overlap));
            }
            j += 1;
        }
        if let Some((j, _)) = best {
            *cell = match items[j].0 {
                None => MapCell::Gap,
                Some(idx) => MapCell::Region { idx, resident: false },
            };
        }
    }

    // Shade the first round(rss/size * width) cells of each region as resident.
    let mut owned: Vec<Vec<usize>> = vec![Vec::new(); regions.len()];
    for (c, cell) in cells.iter().enumerate() {
        if let MapCell::Region { idx, .. } = cell {
            owned[*idx].push(c);
        }
    }
    for (i, list) in owned.iter().enumerate() {
        let r = &regions[i];
        let mut n = (r.resident_fraction() * list.len() as f64).round() as usize;
        if n == 0 && r.rss() > 0 && !list.is_empty() {
            n = 1;
        }
        for &c in list.iter().take(n) {
            if let MapCell::Region { resident, .. } = &mut cells[c] {
                *resident = true;
            }
        }
    }

    for (j, it) in items.iter().enumerate() {
        if let Some(i) = it.0 {
            let mid = ((spans[j].0 + spans[j].1) / 2.0).floor() as usize;
            anchor[i] = owned[i].first().copied().unwrap_or(mid.min(ncells - 1));
        }
    }
    MapLayout { cells, anchor }
}

/// Human-readable binary size: 512B, 4.0K, 1.5M, 128G.
pub fn fmt_bytes(b: u64) -> String {
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b}B")
    } else if v >= 100.0 {
        format!("{v:.0}{}", UNITS[i])
    } else {
        format!("{v:.1}{}", UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(start: u64, end: u64, perms: &str, path: &str) -> Region {
        Region {
            start,
            end,
            perms: perms.into(),
            offset: 0,
            dev: "00:00".into(),
            inode: if path.starts_with('/') { 1 } else { 0 },
            path: path.into(),
            kind: Kind::Anon,
            smaps: None,
        }
    }

    #[test]
    fn classifies_common_regions() {
        let mut r = vec![
            region(0x1000, 0x2000, "r-xp", "/usr/bin/demo"),
            region(0x3000, 0x5000, "rw-p", "[heap]"),
            region(0x10000, 0x11000, "---p", ""),
            region(0x11000, 0x20000, "rw-p", ""),
            region(0x30000, 0x31000, "r-xp", "/usr/lib/x86_64-linux-gnu/libc.so.6"),
            region(0x40000, 0x41000, "rw-s", "/dev/shm/x"),
            region(0x50000, 0x51000, "r--p", "/etc/hosts (deleted)"),
            region(0x60000, 0x61000, "rw-p", ""),
            region(0x61000, 0x62000, "---p", ""),
            region(0x62000, 0x64000, "rw-p", ""),
            region(0x70000, 0x71000, "rw-p", "[stack]"),
            region(0x80000, 0x81000, "r-xp", "[vdso]"),
        ];
        classify(&mut r, Some("/usr/bin/demo"));
        let kinds: Vec<Kind> = r.iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            vec![
                Kind::Exe,
                Kind::Heap,
                Kind::Guard,
                Kind::ThreadStack,
                Kind::Lib,
                Kind::Shm,
                Kind::File,
                Kind::Anon,
                Kind::Guard,
                Kind::Anon, // 8 KiB after a guard: signal stack, not a thread stack
                Kind::Stack,
                Kind::Kernel
            ]
        );
    }

    #[test]
    fn exe_falls_back_to_first_mapping() {
        let mut r = vec![region(0x1000, 0x2000, "r--p", "/opt/app"), region(0x2000, 0x3000, "r-xp", "/opt/app")];
        classify(&mut r, None);
        assert!(r.iter().all(|r| r.kind == Kind::Exe));
    }

    #[test]
    fn map_layout_marks_gaps_and_regions() {
        let r = vec![region(0x1000, 0x2000, "r--p", ""), region(0x9000, 0xa000, "r--p", "")];
        let m = layout_map(&r, 25);
        assert_eq!(m.cells.len(), 25);
        assert!(m.cells.contains(&MapCell::Gap));
        assert!(matches!(m.cells[0], MapCell::Region { idx: 0, .. }));
        assert!(matches!(m.cells[24], MapCell::Region { idx: 1, .. }));
        assert_eq!(m.anchor[0], 0);
    }

    #[test]
    fn map_layout_with_more_regions_than_cells() {
        let r: Vec<Region> = (0..100u64).map(|i| region(i * 0x1000, (i + 1) * 0x1000, "rw-p", "")).collect();
        let m = layout_map(&r, 10);
        assert!(m.cells.iter().all(|c| matches!(c, MapCell::Region { .. })));
        assert!(m.anchor.iter().all(|&a| a < 10));
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(fmt_bytes(512), "512B");
        assert_eq!(fmt_bytes(4096), "4.0K");
        assert_eq!(fmt_bytes(150 * 1024 * 1024), "150M");
    }
}
