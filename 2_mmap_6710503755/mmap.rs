//! mmap - memory map visualizer for Linux processes.
//!
//! Reads /proc/PID/maps, /proc/PID/smaps and /proc/PID/status and shows them either as a
//! real-time full-screen view (default) or as a one-shot text report (--snapshot).
//!
//! Made for an Operating Systems course assignment. Educational use only; not production software.

mod demo;
mod model;
mod procfs;
mod snapshot;
mod ui;

use std::io::IsTerminal;
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "\
mmap - visualize the memory map of a Linux process (educational use only)

USAGE:
  mmap                       open the process picker, then the live view
  mmap -p PID                live view of process PID
  mmap --demo                start a demo process that allocates/frees memory and watch it
  mmap -p PID --snapshot     print a one-shot text report instead of the live view

OPTIONS:
  -p, --pid PID        process to inspect (your own processes; others need root)
  -s, --snapshot       print a text report once and exit
      --demo           inspect a built-in demo process (killed again when mmap exits)
  -i, --interval MS    live view refresh period in milliseconds, 200-10000 (default 1000)
  -w, --width N        snapshot: map cells per line, 40-400 (default 96)
      --no-color       snapshot: plain text without ANSI colors (also when NO_COLOR is set
                       or stdout is not a terminal)
  -h, --help           show this help

LIVE VIEW KEYS:
  arrows/j/k select  Enter details  s sort  r reverse  f filter  t per-kind view
  p pick process  space pause  +/- refresh rate  ? help  q quit   (mouse: wheel, click)
";

struct Options {
    pid: Option<i32>,
    snapshot: bool,
    demo: bool,
    no_color: bool,
    interval_ms: u64,
    width: usize,
}

fn parse_number<T: std::str::FromStr + PartialOrd + std::fmt::Display>(
    flag: &str,
    value: Option<&String>,
    min: T,
    max: T,
) -> Result<T, String> {
    let value = value.ok_or_else(|| format!("{flag} needs a value"))?;
    let n: T = value.parse().map_err(|_| format!("{flag}: '{value}' is not a valid number"))?;
    if n < min || n > max {
        return Err(format!("{flag}: {value} is out of range ({min}-{max})"));
    }
    Ok(n)
}

/// Returns Ok(None) when help was requested.
fn parse_args(args: &[String]) -> Result<Option<Options>, String> {
    let mut o = Options { pid: None, snapshot: false, demo: false, no_color: false, interval_ms: 1000, width: 96 };
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        match flag {
            "-h" | "--help" => return Ok(None),
            "-p" | "--pid" => {
                o.pid = Some(parse_number(flag, args.get(i + 1), 1, i32::MAX)?);
                i += 1;
            }
            "-i" | "--interval" => {
                o.interval_ms = parse_number(flag, args.get(i + 1), 200, 10_000)?;
                i += 1;
            }
            "-w" | "--width" => {
                o.width = parse_number(flag, args.get(i + 1), 40, 400)?;
                i += 1;
            }
            "-s" | "--snapshot" => o.snapshot = true,
            "--demo" => o.demo = true,
            "--no-color" => o.no_color = true,
            _ => return Err(format!("unknown argument '{flag}'")),
        }
        i += 1;
    }
    if o.demo && o.pid.is_some() {
        return Err("use either --demo or -p PID, not both".into());
    }
    Ok(Some(o))
}

fn run(o: Options) -> Result<(), String> {
    if !o.snapshot && !std::io::stdout().is_terminal() {
        return Err("stdout is not a terminal; use --snapshot for a text report".into());
    }
    if let Some(pid) = o.pid {
        procfs::snapshot(pid)?; // fail early with a clear message if the PID does not exist
    }
    let demo = if o.demo {
        Some(demo::spawn().map_err(|e| format!("cannot start the demo process: {e}"))?)
    } else {
        None
    };

    if o.snapshot {
        let pid = o.pid.or(demo.as_ref().map(|d| d.pid() as i32)).ok_or("--snapshot needs -p PID or --demo")?;
        if demo.is_some() {
            // Let the demo allocate a few phases' worth of memory first.
            std::thread::sleep(Duration::from_secs(6));
        }
        let snap = procfs::snapshot(pid)?;
        let color = !o.no_color && std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal();
        print!("{}", snapshot::render(&snap, &procfs::meminfo(), color, o.width));
        return Ok(());
    }
    ui::run(o.pid, Duration::from_millis(o.interval_ms), demo).map_err(|e| format!("terminal error: {e}"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--demo-child") {
        demo::child_main(args.get(1).map(String::as_str));
    }
    let opts = match parse_args(&args) {
        Ok(Some(o)) => o,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("mmap: {e}\nTry 'mmap --help' for more information.");
            return ExitCode::FAILURE;
        }
    };
    match run(opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mmap: {e}");
            ExitCode::FAILURE
        }
    }
}
