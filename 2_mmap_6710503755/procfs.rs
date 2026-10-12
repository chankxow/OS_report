//! Read-only access to the Linux /proc files this tool uses. Nothing here writes to /proc,
//! attaches to a process or needs root.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;

use crate::model::{classify, Region, Smaps};

/// Selected fields of /proc/PID/status. `Vm*`/`Rss*` values are kept in KiB.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub name: String,
    pub state: String,
    pub uid: u32,
    pub threads: u64,
    kib: HashMap<String, u64>,
}

impl Status {
    /// A "Key: N kB" field in bytes, 0 when absent (kernel threads have no Vm* fields).
    pub fn bytes(&self, key: &str) -> u64 {
        self.kib.get(key).copied().unwrap_or(0) * 1024
    }
}

pub fn parse_status(text: &str) -> Status {
    let mut st = Status::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        match key {
            "Name" => st.name = value.to_string(),
            "State" => st.state = value.to_string(),
            "Uid" => st.uid = value.split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "Threads" => st.threads = value.parse().unwrap_or(0),
            _ => {
                if let Some(kb) = value.strip_suffix(" kB").and_then(|v| v.trim().parse().ok()) {
                    st.kib.insert(key.to_string(), kb);
                }
            }
        }
    }
    st
}

/// Splits off the next whitespace-separated token.
fn split_token(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    s.split_at(end)
}

/// Parses one maps line: `start-end perms offset dev inode [path]`.
/// The path is everything after the inode, so names containing spaces survive.
pub fn parse_maps_line(line: &str) -> Option<Region> {
    let (range, rest) = split_token(line);
    let (perms, rest) = split_token(rest);
    let (offset, rest) = split_token(rest);
    let (dev, rest) = split_token(rest);
    let (inode, rest) = split_token(rest);
    let (start, end) = range.split_once('-')?;
    let start = u64::from_str_radix(start, 16).ok()?;
    let end = u64::from_str_radix(end, 16).ok()?;
    if perms.len() != 4 || end < start {
        return None;
    }
    Some(Region {
        start,
        end,
        perms: perms.to_string(),
        offset: u64::from_str_radix(offset, 16).ok()?,
        dev: dev.to_string(),
        inode: inode.parse().ok()?,
        path: rest.trim().to_string(),
        kind: crate::model::Kind::Anon,
        smaps: None,
    })
}

/// Parses /proc/PID/smaps: a maps line per region followed by its "Key: value" lines.
pub fn parse_smaps(text: &str) -> Vec<Region> {
    let mut regions: Vec<Region> = Vec::new();
    for line in text.lines() {
        let first = line.split_whitespace().next().unwrap_or("");
        if let Some(key) = first.strip_suffix(':') {
            if let Some(r) = regions.last_mut() {
                let value = line[first.len()..].trim();
                r.smaps.get_or_insert_with(Smaps::default).set(key, value);
            }
        } else if let Some(r) = parse_maps_line(line) {
            regions.push(r);
        }
    }
    regions
}

/// Everything the views need about one process at one moment.
pub struct Snapshot {
    pub pid: i32,
    pub status: Status,
    pub cmdline: String,
    pub exe: Option<String>,
    pub regions: Vec<Region>,
    /// True when per-region counters (RSS, PSS...) came from smaps.
    pub has_smaps: bool,
    /// Why the map is missing or incomplete, if it is.
    pub map_error: Option<String>,
}

fn read_cmdline(pid: i32) -> String {
    fs::read(format!("/proc/{pid}/cmdline"))
        .map(|b| {
            b.split(|&c| c == 0)
                .filter(|s| !s.is_empty())
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

pub fn snapshot(pid: i32) -> Result<Snapshot, String> {
    let base = format!("/proc/{pid}");
    let status = match fs::read_to_string(format!("{base}/status")) {
        Ok(text) => parse_status(&text),
        Err(e) if e.kind() == ErrorKind::NotFound => return Err(format!("no process with PID {pid}")),
        Err(e) => return Err(format!("cannot read {base}/status: {e}")),
    };
    let mut cmdline = read_cmdline(pid);
    if cmdline.is_empty() {
        cmdline = format!("[{}]", status.name);
    }
    let exe = fs::read_link(format!("{base}/exe")).ok().map(|p| p.to_string_lossy().into_owned());

    let (mut regions, has_smaps, map_error) = match fs::read_to_string(format!("{base}/smaps")) {
        Ok(text) => (parse_smaps(&text), true, None),
        Err(smaps_err) => match fs::read_to_string(format!("{base}/maps")) {
            Ok(text) => (
                text.lines().filter_map(parse_maps_line).collect(),
                false,
                Some(format!("smaps unreadable ({smaps_err}); showing maps without RSS")),
            ),
            Err(e) if e.kind() == ErrorKind::PermissionDenied => (
                Vec::new(),
                false,
                Some(format!(
                    "permission denied: the memory map of PID {pid} (uid {}) is readable only by its owner or root",
                    status.uid
                )),
            ),
            Err(e) if e.kind() == ErrorKind::NotFound => return Err(format!("process {pid} has exited")),
            Err(e) => (Vec::new(), false, Some(format!("cannot read {base}/maps: {e}"))),
        },
    };
    classify(&mut regions, exe.as_deref());
    Ok(Snapshot { pid, status, cmdline, exe, regions, has_smaps, map_error })
}

/// System-wide memory from /proc/meminfo, in bytes.
#[derive(Clone, Copy, Debug, Default)]
pub struct MemInfo {
    pub total: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_free: u64,
}

pub fn meminfo() -> MemInfo {
    let text = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut m = MemInfo::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let kb: u64 = value.trim().trim_end_matches(" kB").trim().parse().unwrap_or(0);
        match key {
            "MemTotal" => m.total = kb * 1024,
            "MemAvailable" => m.available = kb * 1024,
            "SwapTotal" => m.swap_total = kb * 1024,
            "SwapFree" => m.swap_free = kb * 1024,
            _ => {}
        }
    }
    m
}

/// A row of the process picker.
#[derive(Clone, Debug)]
pub struct ProcInfo {
    pub pid: i32,
    pub uid: u32,
    pub name: String,
    pub cmdline: String,
    pub rss: u64,
}

/// All processes, largest RSS first. Processes that exit while being read are skipped.
pub fn list_processes() -> Vec<ProcInfo> {
    let Ok(dir) = fs::read_dir("/proc") else { return Vec::new() };
    let mut procs: Vec<ProcInfo> = dir
        .filter_map(|e| e.ok()?.file_name().to_str()?.parse::<i32>().ok())
        .filter_map(|pid| {
            let st = parse_status(&fs::read_to_string(format!("/proc/{pid}/status")).ok()?);
            Some(ProcInfo { pid, uid: st.uid, rss: st.bytes("VmRSS"), cmdline: read_cmdline(pid), name: st.name })
        })
        .collect();
    procs.sort_by(|a, b| b.rss.cmp(&a.rss).then(a.pid.cmp(&b.pid)));
    procs
}

pub fn current_uid() -> u32 {
    // SAFETY: getuid() has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_maps_lines() {
        let r = parse_maps_line("55d0c1a00000-55d0c1a22000 r--p 00000000 fd:01 1835123    /usr/bin/cat").unwrap();
        assert_eq!((r.start, r.end, r.size()), (0x55d0c1a00000, 0x55d0c1a22000, 0x22000));
        assert_eq!((r.perms.as_str(), r.dev.as_str(), r.inode), ("r--p", "fd:01", 1835123));
        assert_eq!(r.path, "/usr/bin/cat");

        let r = parse_maps_line("7f0000000000-7f0000021000 rw-p 00000000 00:00 0 ").unwrap();
        assert_eq!(r.path, "");

        let r = parse_maps_line("7f00-7f10 r--p 00001000 08:02 42   /tmp/my file.txt (deleted)").unwrap();
        assert_eq!(r.path, "/tmp/my file.txt (deleted)");
        assert_eq!(r.offset, 0x1000);

        let r = parse_maps_line("ffffffffff600000-ffffffffff601000 --xp 00000000 00:00 0  [vsyscall]").unwrap();
        assert_eq!(r.path, "[vsyscall]");

        assert!(parse_maps_line("garbage").is_none());
        assert!(parse_maps_line("").is_none());
    }

    #[test]
    fn parses_smaps() {
        let text = "\
5600-5700 rw-p 00000000 00:00 0                  [heap]
Size:                  4 kB
Rss:                   4 kB
Pss:                   2 kB
Private_Dirty:         4 kB
Swap:                  0 kB
VmFlags: rd wr mr mw me ac
7f00-7f01 r--p 00000000 fd:01 99    /usr/lib/libc.so.6
Size:                  4 kB
Rss:                   0 kB
";
        let r = parse_smaps(text);
        assert_eq!(r.len(), 2);
        let s = r[0].smaps.as_ref().unwrap();
        assert_eq!((s.size, s.rss, s.pss, s.private_dirty), (4, 4, 2, 4));
        assert_eq!(r[0].rss(), 4096);
        assert_eq!(s.raw.last().unwrap(), &("VmFlags".to_string(), "rd wr mr mw me ac".to_string()));
        assert_eq!(r[1].path, "/usr/lib/libc.so.6");
        assert_eq!(r[1].rss(), 0);
    }

    #[test]
    fn parses_status() {
        let st = parse_status("Name:\tcat\nState:\tR (running)\nUid:\t1000\t1000\t1000\t1000\nVmRSS:\t    1234 kB\nThreads:\t3\n");
        assert_eq!(st.name, "cat");
        assert_eq!(st.uid, 1000);
        assert_eq!(st.threads, 3);
        assert_eq!(st.bytes("VmRSS"), 1234 * 1024);
        assert_eq!(st.bytes("VmSwap"), 0);
    }

    #[test]
    fn snapshot_of_self_matches_kernel_totals() {
        let snap = snapshot(std::process::id() as i32).unwrap();
        assert!(snap.has_smaps);
        assert!(snap.regions.iter().any(|r| r.kind == crate::model::Kind::Heap || r.kind == crate::model::Kind::Anon));
        assert!(snap.regions.iter().any(|r| r.kind == crate::model::Kind::Stack));
        assert!(snap.regions.windows(2).all(|w| w[0].end <= w[1].start));
    }

    #[test]
    fn missing_process_is_an_error() {
        assert!(snapshot(i32::MAX).is_err());
    }
}
