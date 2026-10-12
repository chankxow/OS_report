//! `--demo`: a child process that allocates and frees memory in phases so the viewer has
//! something that visibly changes. The child names itself after the current phase
//! (visible in /proc/PID/comm), stays under ~140 MiB, and exits when its parent goes away.

use std::hint::black_box;
use std::io;
use std::process::{Child, Command, Stdio};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const STEP: Duration = Duration::from_millis(800);
/// The demo stops on its own after this long even if nobody kills it.
const MAX_LIFETIME: Duration = Duration::from_secs(30 * 60);
const MIB: usize = 1 << 20;
const PAGE: usize = 4096;

/// Owns the demo child; dropping it (normal exit, error or panic unwinding) kills and reaps it.
pub struct DemoGuard {
    child: Child,
}

impl DemoGuard {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for DemoGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn spawn() -> io::Result<DemoGuard> {
    let child = Command::new(std::env::current_exe()?)
        .arg("--demo-child")
        .arg(std::process::id().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(DemoGuard { child })
}

static PARENT: AtomicU32 = AtomicU32::new(0);

/// Sleeps one step, and exits if the parent has gone (re-parented to init or a subreaper).
fn pause() {
    thread::sleep(STEP);
    if std::os::unix::process::parent_id() != PARENT.load(Ordering::Relaxed) {
        std::process::exit(0);
    }
}

fn set_phase(name: &str) {
    let mut buf = [0u8; 16];
    let n = name.len().min(15);
    buf[..n].copy_from_slice(&name.as_bytes()[..n]);
    // SAFETY: PR_SET_NAME reads a NUL-terminated string of at most 16 bytes from `buf`.
    unsafe {
        libc::prctl(libc::PR_SET_NAME, buf.as_ptr() as libc::c_ulong, 0, 0, 0);
    }
}

pub fn child_main(parent: Option<&str>) -> ! {
    let parent: u32 = parent.and_then(|p| p.parse().ok()).unwrap_or(0);
    PARENT.store(parent, Ordering::Relaxed);
    // SAFETY: plain prctl call; asks the kernel to SIGTERM us when the parent thread dies.
    unsafe {
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM as libc::c_ulong, 0, 0, 0);
    }
    // The parent may have died before prctl took effect.
    if parent == 0 || std::os::unix::process::parent_id() != parent {
        std::process::exit(0);
    }
    let started = Instant::now();
    while started.elapsed() < MAX_LIFETIME {
        cycle();
    }
    std::process::exit(0)
}

/// An anonymous or file mapping created with mmap(2), unmapped on drop.
struct Mapping {
    ptr: *mut libc::c_void,
    len: usize,
}

impl Mapping {
    fn anonymous(len: usize) -> Option<Mapping> {
        // SAFETY: requesting a fresh private anonymous mapping; the result is checked.
        let ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        (ptr != libc::MAP_FAILED).then_some(Mapping { ptr, len })
    }

    /// Maps the first readable file from a short list of harmless system files, read-only.
    fn some_file() -> Option<Mapping> {
        use std::os::unix::io::AsRawFd;
        const CANDIDATES: [&str; 3] = ["/etc/ssl/certs/ca-certificates.crt", "/etc/services", "/etc/passwd"];
        CANDIDATES.iter().find_map(|path| {
            let f = std::fs::File::open(path).ok()?;
            let len = (f.metadata().ok()?.len() as usize).min(16 * MIB);
            if len == 0 {
                return None;
            }
            // SAFETY: read-only private mapping of an open file; the result is checked.
            // The mapping stays valid after `f` is closed.
            let ptr = unsafe { libc::mmap(ptr::null_mut(), len, libc::PROT_READ, libc::MAP_PRIVATE, f.as_raw_fd(), 0) };
            (ptr != libc::MAP_FAILED).then_some(Mapping { ptr, len })
        })
    }

    /// Writes one byte per page in [from, to) so those pages become resident.
    fn touch(&self, from: usize, to: usize) {
        for off in (from..to.min(self.len)).step_by(PAGE) {
            // SAFETY: off < len and the mapping is writable (only called on anonymous mappings).
            unsafe { *(self.ptr as *mut u8).add(off) = 1 };
        }
    }

    /// Reads one byte per page so the file's pages are faulted in.
    fn read_all(&self) -> u64 {
        (0..self.len)
            .step_by(PAGE)
            // SAFETY: off < len and the mapping is readable.
            .map(|off| unsafe { *(self.ptr as *const u8).add(off) } as u64)
            .sum()
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: ptr/len are exactly what mmap returned.
        unsafe {
            libc::munmap(self.ptr, self.len);
        }
    }
}

/// Recurses with a 64 KiB frame per level, then waits at the bottom: the [stack] mapping grows.
#[inline(never)]
fn grow_stack(depth: u32) -> u64 {
    let mut frame = [0u8; 64 * 1024];
    frame[depth as usize * 997 % frame.len()] = depth as u8;
    black_box(&mut frame);
    let below = if depth == 0 {
        pause();
        pause();
        pause();
        0
    } else {
        grow_stack(depth - 1)
    };
    below + frame[0] as u64
}

fn cycle() {
    // 1. Many small blocks: glibc serves requests < 128 KiB from [heap], grown with brk().
    set_phase("demo:heap");
    let mut small: Vec<Box<[u8]>> = Vec::new();
    for _ in 0..4 {
        for _ in 0..50 {
            small.push(vec![0xA5u8; 16 * 1024].into_boxed_slice());
        }
        pause();
    }

    // 2. Large blocks: glibc gives each request >= 128 KiB its own anonymous mmap.
    set_phase("demo:bigvec");
    let mut big: Vec<Vec<u8>> = Vec::new();
    for _ in 0..2 {
        big.push(vec![0x5Au8; 32 * MIB]);
        pause();
    }

    // 3. One 64 MiB mapping touched 8 MiB at a time: VmSize jumps at once, RSS grows step by step.
    set_phase("demo:mmap");
    let anon = Mapping::anonymous(64 * MIB);
    if let Some(m) = &anon {
        for step in 0..8 {
            m.touch(step * 8 * MIB, (step + 1) * 8 * MIB);
            pause();
        }
    }

    // 4. A read-only file mapping.
    set_phase("demo:file");
    let file = Mapping::some_file();
    if let Some(f) = &file {
        black_box(f.read_all());
    }
    pause();
    pause();

    // 5. Two threads, each with a 1 MiB stack below a guard page.
    set_phase("demo:threads");
    let stop = Arc::new(AtomicBool::new(false));
    let workers: Vec<_> = (0..2)
        .filter_map(|i| {
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name(format!("demo-worker-{i}"))
                .stack_size(MIB)
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(100));
                    }
                })
                .ok()
        })
        .collect();
    pause();
    pause();

    // 6. Deep recursion: ~1.5 MiB of the main stack. The kernel never shrinks [stack] afterwards.
    set_phase("demo:stack");
    black_box(grow_stack(24));

    // 7. Give everything back, one kind at a time.
    set_phase("demo:free");
    stop.store(true, Ordering::Relaxed);
    for w in workers {
        let _ = w.join();
    }
    pause();
    drop(big);
    pause();
    drop(anon);
    drop(file);
    pause();
    drop(small);
    // SAFETY: malloc_trim only returns free heap memory to the OS.
    unsafe {
        libc::malloc_trim(0);
    }
    pause();
    pause();
}
