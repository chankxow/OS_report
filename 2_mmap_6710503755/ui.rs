//! Real-time full-screen view in the style of btop: memory meters, an RSS history chart,
//! a colored address-space map and a sortable region table, refreshed from /proc.

use std::io;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, BorderType, Cell, Chart, Clear, Dataset, GraphType, HighlightSpacing, Paragraph, Row, Table,
    TableState,
};
use ratatui::{DefaultTerminal, Frame};

use crate::demo::DemoGuard;
use crate::model::{fmt_bytes, layout_map, summarize, History, Kind, MapCell, Region, Sample};
use crate::procfs::{self, MemInfo, ProcInfo, Snapshot};

const BORDER: Color = Color::Rgb(88, 101, 140);
const TITLE: Color = Color::Rgb(225, 228, 245);
const TEXT: Color = Color::Rgb(200, 204, 220);
const DIM: Color = Color::Rgb(115, 122, 145);
const TRACK: Color = Color::Rgb(48, 52, 68);
const ACCENT: Color = Color::Rgb(255, 184, 108);
const SEL_BG: Color = Color::Rgb(58, 63, 92);
const ERROR: Color = Color::Rgb(255, 85, 85);
const RSS_C: Color = Color::Rgb(80, 250, 123);
const ANON_C: Color = Color::Rgb(255, 110, 110);
const FILE_C: Color = Color::Rgb(139, 233, 253);
const SHM_C: Color = Color::Rgb(100, 149, 237);
const SWAP_C: Color = Color::Rgb(189, 147, 249);

/// Refresh intervals that + and - step through, in milliseconds.
const INTERVALS: [u64; 7] = [200, 500, 1000, 2000, 3000, 5000, 10000];
/// Seconds of history shown in the chart.
const WINDOW: f64 = 60.0;

#[derive(Clone, Copy, PartialEq)]
enum SortKey {
    Address,
    Size,
    Rss,
    Pss,
    Dirty,
    Swap,
}

impl SortKey {
    fn next(self) -> SortKey {
        match self {
            SortKey::Address => SortKey::Size,
            SortKey::Size => SortKey::Rss,
            SortKey::Rss => SortKey::Pss,
            SortKey::Pss => SortKey::Dirty,
            SortKey::Dirty => SortKey::Swap,
            SortKey::Swap => SortKey::Address,
        }
    }

    fn label(self) -> &'static str {
        match self {
            SortKey::Address => "address",
            SortKey::Size => "size",
            SortKey::Rss => "rss",
            SortKey::Pss => "pss",
            SortKey::Dirty => "dirty",
            SortKey::Swap => "swap",
        }
    }

    fn key(self, r: &Region) -> u64 {
        match self {
            SortKey::Address => r.start,
            SortKey::Size => r.size(),
            SortKey::Rss => r.rss(),
            SortKey::Pss => r.pss(),
            SortKey::Dirty => r.dirty(),
            SortKey::Swap => r.swap(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Regions,
    Kinds,
}

struct Picker {
    all: Vec<ProcInfo>,
    query: String,
    state: TableState,
}

impl Picker {
    fn new(current: Option<i32>) -> Picker {
        let all = procfs::list_processes();
        let sel = current.and_then(|pid| all.iter().position(|p| p.pid == pid)).unwrap_or(0);
        Picker { all, query: String::new(), state: TableState::default().with_selected(Some(sel)) }
    }

    fn filtered(&self) -> Vec<&ProcInfo> {
        let q = self.query.to_lowercase();
        self.all
            .iter()
            .filter(|p| {
                q.is_empty()
                    || p.pid.to_string().starts_with(&q)
                    || p.name.to_lowercase().contains(&q)
                    || p.cmdline.to_lowercase().contains(&q)
            })
            .collect()
    }

    fn move_by(&mut self, delta: isize) {
        let n = self.filtered().len();
        if n == 0 {
            self.state.select(None);
            return;
        }
        let cur = self.state.selected().unwrap_or(0) as isize;
        self.state.select(Some((cur + delta).clamp(0, n as isize - 1) as usize));
    }
}

pub struct App {
    pid: Option<i32>,
    demo_pid: Option<i32>,
    uid: u32,
    snap: Option<Snapshot>,
    error: Option<String>,
    mem: MemInfo,
    history: History,
    started: Instant,
    last_refresh: Option<Instant>,
    interval: Duration,
    paused: bool,
    sort: SortKey,
    sort_desc: bool,
    filter: Option<Kind>,
    view: View,
    /// Indices into `snap.regions`, filtered and sorted as shown in the table.
    visible: Vec<usize>,
    /// The selection follows a region (by start address) across refreshes and re-sorts.
    selected_start: Option<u64>,
    table: TableState,
    show_details: bool,
    show_help: bool,
    picker: Option<Picker>,
    // Where things were drawn, for mouse hit-testing.
    table_body: Rect,
    map_area: Rect,
    map_cells: Vec<Option<usize>>,
}

impl App {
    fn new(pid: Option<i32>, interval: Duration, demo_pid: Option<i32>) -> App {
        let mut app = App {
            pid,
            demo_pid,
            uid: procfs::current_uid(),
            snap: None,
            error: None,
            mem: MemInfo::default(),
            history: History::new(600),
            started: Instant::now(),
            last_refresh: None,
            interval,
            paused: false,
            sort: SortKey::Address,
            sort_desc: false,
            filter: None,
            view: View::Regions,
            visible: Vec::new(),
            selected_start: None,
            table: TableState::default(),
            show_details: false,
            show_help: false,
            picker: None,
            table_body: Rect::default(),
            map_area: Rect::default(),
            map_cells: Vec::new(),
        };
        if pid.is_none() {
            app.picker = Some(Picker::new(None));
        }
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        self.last_refresh = Some(Instant::now());
        self.mem = procfs::meminfo();
        let Some(pid) = self.pid else { return };
        match procfs::snapshot(pid) {
            Ok(snap) => {
                let st = &snap.status;
                let mib = |key: &str| st.bytes(key) as f64 / (1024.0 * 1024.0);
                self.history.push(Sample {
                    t: self.started.elapsed().as_secs_f64(),
                    rss: mib("VmRSS"),
                    anon: mib("RssAnon"),
                    file: mib("RssFile"),
                });
                self.error = snap.map_error.clone();
                self.snap = Some(snap);
                self.rebuild_visible();
            }
            Err(e) => {
                let seen_before = self.snap.as_ref().is_some_and(|s| s.pid == pid);
                self.error = Some(if seen_before {
                    format!("process {pid} has exited; showing its last state (press p to pick another)")
                } else {
                    format!("{e} (press p to pick another process)")
                });
            }
        }
    }

    fn set_pid(&mut self, pid: i32) {
        if self.pid != Some(pid) {
            self.pid = Some(pid);
            self.snap = None;
            self.history.clear();
            self.started = Instant::now();
            self.selected_start = None;
            self.filter = None;
            self.show_details = false;
        }
        self.refresh();
    }

    fn rebuild_visible(&mut self) {
        let Some(snap) = &self.snap else {
            self.visible.clear();
            return;
        };
        let (sort, desc, filter) = (self.sort, self.sort_desc, self.filter);
        let mut v: Vec<usize> =
            (0..snap.regions.len()).filter(|&i| filter.map_or(true, |k| snap.regions[i].kind == k)).collect();
        v.sort_by_key(|&i| sort.key(&snap.regions[i]));
        if desc {
            v.reverse();
        }
        let pos = self.selected_start.and_then(|s| v.iter().position(|&i| snap.regions[i].start == s));
        self.visible = v;
        self.select(pos.unwrap_or(0));
    }

    fn select(&mut self, idx: usize) {
        let Some(snap) = &self.snap else { return };
        if self.visible.is_empty() {
            self.table.select(None);
            self.selected_start = None;
            return;
        }
        let idx = idx.min(self.visible.len() - 1);
        self.table.select(Some(idx));
        self.selected_start = Some(snap.regions[self.visible[idx]].start);
    }

    fn move_selection(&mut self, delta: isize) {
        let cur = self.table.selected().unwrap_or(0) as isize;
        self.select((cur + delta).max(0) as usize);
    }

    fn selected_region(&self) -> Option<&Region> {
        let snap = self.snap.as_ref()?;
        let i = *self.visible.get(self.table.selected()?)?;
        snap.regions.get(i)
    }

    fn select_region_index(&mut self, ri: usize) {
        let Some(snap) = &self.snap else { return };
        if self.filter.is_some_and(|k| snap.regions[ri].kind != k) {
            self.filter = None;
            self.rebuild_visible();
        }
        if let Some(pos) = self.visible.iter().position(|&i| i == ri) {
            self.select(pos);
        }
    }

    fn cycle_filter(&mut self) {
        let Some(snap) = &self.snap else { return };
        let present: Vec<Kind> = summarize(&snap.regions).iter().map(|s| s.kind).collect();
        self.filter = match self.filter {
            None => present.first().copied(),
            Some(k) => present.iter().skip_while(|&&p| p != k).nth(1).copied(),
        };
        self.rebuild_visible();
    }

    fn step_interval(&mut self, up: bool) {
        let ms = self.interval.as_millis() as u64;
        let next = if up {
            INTERVALS.iter().copied().find(|&v| v > ms).unwrap_or(ms)
        } else {
            INTERVALS.iter().rev().copied().find(|&v| v < ms).unwrap_or(ms)
        };
        self.interval = Duration::from_millis(next);
    }

    fn page(&self) -> isize {
        self.table_body.height.max(1) as isize
    }

    /// Returns true when the app should quit.
    fn on_key(&mut self, k: KeyEvent) -> bool {
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            return true;
        }
        if self.picker.is_some() {
            return self.on_picker_key(k);
        }
        if self.show_help {
            self.show_help = false;
            return false;
        }
        if self.show_details {
            match k.code {
                KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
                _ => self.show_details = false,
            }
            return false;
        }
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-self.page()),
            KeyCode::PageDown => self.move_selection(self.page()),
            KeyCode::Home | KeyCode::Char('g') => self.select(0),
            KeyCode::End | KeyCode::Char('G') => self.select(usize::MAX),
            KeyCode::Enter => self.show_details = self.selected_region().is_some(),
            KeyCode::Char('s') => {
                self.sort = self.sort.next();
                self.sort_desc = self.sort != SortKey::Address;
                self.rebuild_visible();
            }
            KeyCode::Char('r') => {
                self.sort_desc = !self.sort_desc;
                self.rebuild_visible();
            }
            KeyCode::Char('f') => self.cycle_filter(),
            KeyCode::Char('t') | KeyCode::Tab => {
                self.view = if self.view == View::Regions { View::Kinds } else { View::Regions };
            }
            KeyCode::Char('p') => self.picker = Some(Picker::new(self.pid)),
            KeyCode::Char(' ') => self.paused = !self.paused,
            KeyCode::Char('+') | KeyCode::Char('=') => self.step_interval(true),
            KeyCode::Char('-') => self.step_interval(false),
            KeyCode::Char('u') => self.refresh(),
            KeyCode::Char('?') | KeyCode::Char('h') | KeyCode::F(1) => self.show_help = true,
            _ => {}
        }
        false
    }

    fn on_picker_key(&mut self, k: KeyEvent) -> bool {
        let Some(p) = self.picker.as_mut() else { return false };
        match k.code {
            KeyCode::Esc => {
                self.picker = None;
                return self.pid.is_none(); // nothing to go back to
            }
            KeyCode::Enter => {
                let chosen = p.state.selected().and_then(|i| p.filtered().get(i).map(|x| x.pid));
                if let Some(pid) = chosen {
                    self.picker = None;
                    self.set_pid(pid);
                }
            }
            KeyCode::Up => p.move_by(-1),
            KeyCode::Down => p.move_by(1),
            KeyCode::PageUp => p.move_by(-10),
            KeyCode::PageDown => p.move_by(10),
            KeyCode::Backspace => {
                p.query.pop();
                p.state.select(Some(0));
            }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                p.query.push(c);
                p.state.select(Some(0));
            }
            _ => {}
        }
        false
    }

    fn on_mouse(&mut self, m: MouseEvent) {
        if let Some(p) = self.picker.as_mut() {
            match m.kind {
                MouseEventKind::ScrollDown => p.move_by(3),
                MouseEventKind::ScrollUp => p.move_by(-3),
                _ => {}
            }
            return;
        }
        if self.show_help || self.show_details {
            if let MouseEventKind::Down(_) = m.kind {
                self.show_help = false;
                self.show_details = false;
            }
            return;
        }
        match m.kind {
            MouseEventKind::ScrollDown => self.move_selection(3),
            MouseEventKind::ScrollUp => self.move_selection(-3),
            MouseEventKind::Down(MouseButton::Left) => {
                let inside = |r: Rect| m.column >= r.x && m.column < r.x + r.width && m.row >= r.y && m.row < r.y + r.height;
                if self.view == View::Regions && inside(self.table_body) {
                    let i = self.table.offset() + (m.row - self.table_body.y) as usize;
                    if i < self.visible.len() {
                        if self.table.selected() == Some(i) {
                            self.show_details = true;
                        } else {
                            self.select(i);
                        }
                    }
                } else if inside(self.map_area) {
                    let c = (m.column - self.map_area.x) as usize;
                    if let Some(Some(ri)) = self.map_cells.get(c).copied() {
                        self.view = View::Regions;
                        self.select_region_index(ri);
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn run(pid: Option<i32>, interval: Duration, demo: Option<DemoGuard>) -> io::Result<()> {
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    // ratatui::init already restores the screen on panic; also switch mouse reporting off.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(io::stdout(), DisableMouseCapture);
        prev_hook(info);
    }));

    let demo_pid = demo.as_ref().map(|d| d.pid() as i32);
    let mut app = App::new(pid.or(demo_pid), interval, demo_pid);
    let result = event_loop(&mut terminal, &mut app);

    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    drop(demo); // kills the demo child, if any
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        let due = app.last_refresh.map_or(true, |t| t.elapsed() >= app.interval);
        if !app.paused && due {
            app.refresh();
        }
        terminal.draw(|f| draw(f, app))?;

        let until_refresh = app.last_refresh.map_or(Duration::ZERO, |t| app.interval.saturating_sub(t.elapsed()));
        let timeout = if app.paused { Duration::from_millis(250) } else { until_refresh.min(Duration::from_millis(250)) };
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => {
                    if app.on_key(k) {
                        return Ok(());
                    }
                }
                Event::Mouse(m) => app.on_mouse(m),
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------- drawing

fn panel(title: String) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(BORDER))
        .title(Line::from(vec![
            Span::styled("┤", Style::new().fg(BORDER)),
            Span::styled(title, Style::new().fg(TITLE).add_modifier(Modifier::BOLD)),
            Span::styled("├", Style::new().fg(BORDER)),
        ]))
}

fn hint(key: &str, text: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(key.to_string(), Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {text}  "), Style::new().fg(DIM)),
    ]
}

fn centered(area: Rect, pct_w: u16, pct_h: u16) -> Rect {
    let w = (area.width * pct_w / 100).max(20).min(area.width);
    let h = (area.height * pct_h / 100).max(8).min(area.height);
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// btop-style meter: label, a bar of ■ cells, then the value.
fn meter(label: &str, frac: f64, value: String, color: Color, width: u16) -> Line<'static> {
    let label_w = 8usize;
    let value_w = value.chars().count() + 1;
    let bar_w = (width as usize).saturating_sub(label_w + value_w);
    let filled = ((frac.clamp(0.0, 1.0) * bar_w as f64).round() as usize).min(bar_w);
    Line::from(vec![
        Span::styled(format!("{label:<label_w$}"), Style::new().fg(TEXT)),
        Span::styled("■".repeat(filled), Style::new().fg(color)),
        Span::styled("■".repeat(bar_w - filled), Style::new().fg(TRACK)),
        Span::styled(format!(" {value}"), Style::new().fg(TITLE)),
    ])
}

fn ratio(a: u64, b: u64) -> f64 {
    if b == 0 {
        0.0
    } else {
        a as f64 / b as f64
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    if area.width < 70 || area.height < 22 {
        let msg = format!("terminal too small: {}x{} (need at least 70x22) — q to quit", area.width, area.height);
        f.render_widget(Paragraph::new(msg).style(Style::new().fg(ACCENT)), area);
        return;
    }
    let [header, top, map, bottom] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(9), Constraint::Length(6), Constraint::Min(6)])
            .areas(area);
    let [mem, hist] = Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)]).areas(top);

    draw_header(f, header, app);
    draw_memory(f, mem, app);
    draw_history(f, hist, app);
    let (map_area, map_cells) = draw_map(f, map, app);
    app.map_area = map_area;
    app.map_cells = map_cells;
    match app.view {
        View::Regions => draw_regions(f, bottom, app),
        View::Kinds => draw_kinds(f, bottom, app),
    }
    if app.show_details {
        draw_details(f, area, app);
    }
    if app.show_help {
        draw_help(f, area);
    }
    let uid = app.uid;
    if let Some(p) = app.picker.as_mut() {
        draw_picker(f, area, p, uid);
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let mut left = vec![Span::styled(" mmap ", Style::new().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD))];
    if let Some(snap) = &app.snap {
        left.push(Span::styled(format!(" pid {} ", snap.pid), Style::new().fg(TITLE).add_modifier(Modifier::BOLD)));
        left.push(Span::styled(snap.status.name.clone(), Style::new().fg(ACCENT)));
        if app.demo_pid == Some(snap.pid) {
            left.push(Span::styled(" (demo)", Style::new().fg(DIM)));
        }
        let room = (area.width as usize).saturating_sub(60);
        left.push(Span::styled(format!("  {}", truncate(&snap.cmdline, room)), Style::new().fg(DIM)));
    } else {
        left.push(Span::styled(" no process selected", Style::new().fg(DIM)));
    }
    f.render_widget(Paragraph::new(Line::from(left)), area);

    let state = if app.paused {
        Span::styled("⏸ paused ", Style::new().fg(ERROR).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(format!("⟳ {:.1}s ", app.interval.as_secs_f64()), Style::new().fg(RSS_C))
    };
    let right = Line::from(vec![state, Span::styled("? help  q quit ", Style::new().fg(DIM))]);
    f.render_widget(Paragraph::new(right).alignment(Alignment::Right), area);
}

fn draw_memory(f: &mut Frame, area: Rect, app: &App) {
    let block = panel("memory".into());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width;
    let m = &app.mem;
    let sys_used = m.total.saturating_sub(m.available);
    let sys_line = meter(
        "system",
        ratio(sys_used, m.total),
        format!("{}/{}", fmt_bytes(sys_used), fmt_bytes(m.total)),
        ACCENT,
        w,
    );
    let Some(snap) = &app.snap else {
        let lines = vec![Line::styled("press p to pick a process", Style::new().fg(DIM)), sys_line];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    };
    let st = &snap.status;
    let (rss, vsz) = (st.bytes("VmRSS"), st.bytes("VmSize"));
    let (anon, file, shm, swap) = (st.bytes("RssAnon"), st.bytes("RssFile"), st.bytes("RssShmem"), st.bytes("VmSwap"));
    let lines = vec![
        meter("rss", ratio(rss, vsz), format!("{} of {} virt", fmt_bytes(rss), fmt_bytes(vsz)), RSS_C, w),
        meter(" anon", ratio(anon, rss), fmt_bytes(anon), ANON_C, w),
        meter(" file", ratio(file, rss), fmt_bytes(file), FILE_C, w),
        meter(" shmem", ratio(shm, rss), fmt_bytes(shm), SHM_C, w),
        meter("swap", ratio(swap, rss + swap), fmt_bytes(swap), SWAP_C, w),
        sys_line,
        Line::from(vec![
            Span::styled("peak ", Style::new().fg(DIM)),
            Span::styled(format!("rss {} virt {}", fmt_bytes(st.bytes("VmHWM")), fmt_bytes(st.bytes("VmPeak"))), Style::new().fg(TEXT)),
            Span::styled("  thr ", Style::new().fg(DIM)),
            Span::styled(st.threads.to_string(), Style::new().fg(TEXT)),
            Span::styled("  maps ", Style::new().fg(DIM)),
            Span::styled(snap.regions.len().to_string(), Style::new().fg(TEXT)),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn series<'a>(name: &'static str, data: &'a [(f64, f64)], color: Color) -> Dataset<'a> {
    Dataset::default().name(name).marker(Marker::Braille).graph_type(GraphType::Line).style(Style::new().fg(color)).data(data)
}

fn draw_history(f: &mut Frame, area: Rect, app: &App) {
    let now = app.history.samples.back().map_or(0.0, |s| s.t).max(WINDOW);
    let x0 = now - WINDOW;
    let pts = |g: fn(&Sample) -> f64| -> Vec<(f64, f64)> {
        app.history.samples.iter().filter(|s| s.t >= x0).map(|s| (s.t, g(s))).collect()
    };
    let (rss, anon, file) = (pts(|s| s.rss), pts(|s| s.anon), pts(|s| s.file));
    let ymax = rss.iter().map(|p| p.1).fold(1.0f64, f64::max) * 1.15;
    let mib = |v: f64| fmt_bytes((v * 1024.0 * 1024.0) as u64);
    let datasets = vec![series("rss", &rss, RSS_C), series("anon", &anon, ANON_C), series("file", &file, FILE_C)];
    // The legend lives in the title: inside the plot it would hide the first minute of data.
    let title = Line::from(vec![
        Span::styled("┤", Style::new().fg(BORDER)),
        Span::styled(format!("rss history · last {WINDOW:.0}s "), Style::new().fg(TITLE).add_modifier(Modifier::BOLD)),
        Span::styled("── rss ", Style::new().fg(RSS_C)),
        Span::styled("── anon ", Style::new().fg(ANON_C)),
        Span::styled("── file", Style::new().fg(FILE_C)),
        Span::styled("├", Style::new().fg(BORDER)),
    ]);
    let block = Block::bordered().border_type(BorderType::Rounded).border_style(Style::new().fg(BORDER)).title(title);
    let chart = Chart::new(datasets)
        .block(block)
        .x_axis(
            Axis::default()
                .bounds([x0, now])
                .style(Style::new().fg(DIM))
                .labels(vec![format!("-{WINDOW:.0}s"), "now".to_string()]),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, ymax])
                .style(Style::new().fg(DIM))
                .labels(vec!["0".to_string(), mib(ymax / 2.0), mib(ymax)]),
        )
        .legend_position(None);
    f.render_widget(chart, area);
}

/// Draws the address-space map; returns the clickable bar area and the region under each cell.
fn draw_map(f: &mut Frame, area: Rect, app: &App) -> (Rect, Vec<Option<usize>>) {
    let sel = app.selected_region().map(|r| r.start);
    let mut block = panel("address space · low → high · width ∝ log₂(size) · gaps collapsed".into());
    if let Some(r) = app.selected_region() {
        let name = r.short_name();
        let what = if name == r.kind.label() { name } else { format!("{} {name}", r.kind.label()) };
        block = block.title_bottom(
            Line::from(vec![
                Span::styled("┤▲ ", Style::new().fg(BORDER)),
                Span::styled(format!("{:x}-{:x} ", r.start, r.end), Style::new().fg(TITLE)),
                Span::styled(format!("{what} "), Style::new().fg(r.kind.color())),
                Span::styled(format!("{} ", fmt_bytes(r.size())), Style::new().fg(TEXT)),
                Span::styled("├", Style::new().fg(BORDER)),
            ])
            .right_aligned(),
        );
    }
    if let Some(e) = &app.error {
        block = block.title_bottom(Line::styled(format!("┤ {e} ├"), Style::new().fg(ERROR)).left_aligned());
    }
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(snap) = &app.snap else { return (Rect::default(), Vec::new()) };
    if snap.regions.is_empty() {
        let msg = if app.error.is_some() { "memory map unavailable" } else { "no user-space mappings (kernel thread?)" };
        f.render_widget(Paragraph::new(msg).style(Style::new().fg(DIM)), inner);
        return (Rect::default(), Vec::new());
    }
    let width = inner.width as usize;
    let layout = layout_map(&snap.regions, width);
    let sel_idx = sel.and_then(|s| snap.regions.iter().position(|r| r.start == s));

    // Two identical bar rows, with region names written into runs that are wide enough.
    let mut bar: Vec<(char, Style)> = layout
        .cells
        .iter()
        .map(|cell| match *cell {
            MapCell::Empty => (' ', Style::new()),
            MapCell::Gap => ('┊', Style::new().fg(TRACK)),
            MapCell::Region { idx, resident } => {
                let k = snap.regions[idx].kind;
                let bg = if resident { k.color() } else { k.dim_color() };
                (' ', Style::new().bg(bg).fg(Color::Rgb(20, 20, 30)))
            }
        })
        .collect();
    let mut c = 0;
    while c < width {
        if let MapCell::Region { idx, .. } = layout.cells[c] {
            let run = layout.cells[c..].iter().take_while(|x| matches!(x, MapCell::Region { idx: i, .. } if *i == idx)).count();
            let name = snap.regions[idx].short_name();
            if run >= name.chars().count() + 2 {
                for (j, ch) in name.chars().enumerate() {
                    bar[c + 1 + j].0 = ch;
                }
            }
            c += run.max(1);
        } else {
            c += 1;
        }
    }
    let to_line = |cells: &[(char, Style)]| Line::from(cells.iter().map(|(ch, st)| Span::styled(ch.to_string(), *st)).collect::<Vec<_>>());
    let plain: Vec<(char, Style)> = bar.iter().map(|(ch, st)| (if *ch == '┊' { '┊' } else { ' ' }, *st)).collect();

    // Pointer row under the selected region.
    let mut pointer = vec![(' ', Style::new()); width];
    if let Some(si) = sel_idx {
        let marks: Vec<usize> = (0..width).filter(|&c| matches!(layout.cells[c], MapCell::Region { idx, .. } if idx == si)).collect();
        let marks = if marks.is_empty() { vec![layout.anchor[si]] } else { marks };
        for c in marks {
            pointer[c] = ('▲', Style::new().fg(ACCENT).add_modifier(Modifier::BOLD));
        }
    }

    let present: Vec<Kind> = summarize(&snap.regions).iter().map(|s| s.kind).collect();
    let mut legend: Vec<Span> = Vec::new();
    for k in present {
        legend.push(Span::styled("■", Style::new().fg(k.color())));
        legend.push(Span::styled(format!("{} ", k.label()), Style::new().fg(TEXT)));
    }
    legend.push(Span::styled(" bright=resident dim=not in RAM ┊=gap", Style::new().fg(DIM)));

    let lines = vec![to_line(&bar), to_line(&plain), to_line(&pointer), Line::from(legend)];
    f.render_widget(Paragraph::new(lines), inner);

    let cells = layout.cells.iter().map(|c| if let MapCell::Region { idx, .. } = c { Some(*idx) } else { None }).collect();
    (Rect { height: 2.min(inner.height), ..inner }, cells)
}

fn draw_regions(f: &mut Frame, area: Rect, app: &mut App) {
    let arrow = if app.sort_desc { "▼" } else { "▲" };
    let filter = app.filter.map_or("all".to_string(), |k| k.label().to_string());
    let mut hints = vec![Span::styled("┤", Style::new().fg(BORDER))];
    for (k, t) in [("↑↓", "select"), ("⏎", "details"), ("s", "sort"), ("r", "reverse"), ("f", "filter"), ("t", "by kind"), ("p", "process"), ("␣", "pause"), ("+-", "speed")] {
        hints.extend(hint(k, t));
    }
    hints.push(Span::styled("├", Style::new().fg(BORDER)));
    let block = panel(format!("regions · {} shown · sort {} {arrow} · filter {filter}", app.visible.len(), app.sort.label()))
        .title_bottom(Line::from(hints));

    let headers = ["START", "END", "SIZE", "PERM", "RSS", "RES%", "PSS", "DIRTY", "SWAP", "KIND", "PATH"];
    let sorted_col = match app.sort {
        SortKey::Address => 0,
        SortKey::Size => 2,
        SortKey::Rss => 4,
        SortKey::Pss => 6,
        SortKey::Dirty => 7,
        SortKey::Swap => 8,
    };
    let header = Row::new(headers.iter().enumerate().map(|(i, h)| {
        let text = if i == sorted_col { format!("{h}{arrow}") } else { h.to_string() };
        let color = if i == sorted_col { ACCENT } else { TITLE };
        Cell::from(text).style(Style::new().fg(color).add_modifier(Modifier::BOLD))
    }));

    let rows: Vec<Row> = match &app.snap {
        None => Vec::new(),
        Some(snap) => app
            .visible
            .iter()
            .map(|&i| {
                let r = &snap.regions[i];
                let have = r.smaps.is_some();
                let stat = |v: u64| if have { fmt_bytes(v) } else { "-".to_string() };
                let frac = r.resident_fraction();
                let filled = (frac * 5.0).round() as usize;
                let res = if have {
                    Line::from(vec![
                        Span::styled("■".repeat(filled), Style::new().fg(r.kind.color())),
                        Span::styled("■".repeat(5 - filled), Style::new().fg(TRACK)),
                        Span::styled(format!("{:>4.0}", frac * 100.0), Style::new().fg(TEXT)),
                    ])
                } else {
                    Line::from("-")
                };
                Row::new(vec![
                    Cell::from(format!("{:x}", r.start)),
                    Cell::from(format!("{:x}", r.end)),
                    Cell::from(fmt_bytes(r.size())),
                    Cell::from(r.perms.clone()),
                    Cell::from(stat(r.rss())),
                    Cell::from(res),
                    Cell::from(stat(r.pss())),
                    Cell::from(stat(r.dirty())),
                    Cell::from(stat(r.swap())),
                    Cell::from(r.kind.label()).style(Style::new().fg(r.kind.color())),
                    Cell::from(r.path.clone()),
                ])
                .style(Style::new().fg(TEXT))
            })
            .collect(),
    };
    let widths = [
        Constraint::Length(16),
        Constraint::Length(16),
        Constraint::Length(7),
        Constraint::Length(4),
        Constraint::Length(7),
        Constraint::Length(9),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(7),
        Constraint::Length(9),
        Constraint::Min(10),
    ];
    let inner = block.inner(area);
    app.table_body = Rect { y: inner.y + 1, height: inner.height.saturating_sub(1), ..inner };
    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .column_spacing(1)
        .row_highlight_style(Style::new().bg(SEL_BG).add_modifier(Modifier::BOLD))
        .highlight_symbol(Span::styled("▶", Style::new().fg(ACCENT)))
        .highlight_spacing(HighlightSpacing::Always);
    f.render_stateful_widget(table, area, &mut app.table);
}

fn draw_kinds(f: &mut Frame, area: Rect, app: &mut App) {
    let block = panel("by kind · share of the process RSS".into())
        .title_bottom(Line::from([vec![Span::styled("┤", Style::new().fg(BORDER))], hint("t", "back to regions"), vec![Span::styled("├", Style::new().fg(BORDER))]].concat()));
    let inner = block.inner(area);
    app.table_body = Rect::default();
    let Some(snap) = &app.snap else {
        f.render_widget(block, area);
        return;
    };
    let stats = summarize(&snap.regions);
    let total_rss: u64 = stats.iter().map(|s| s.rss).sum();
    let bar_w = (inner.width as usize).saturating_sub(11 + 8 + 9 * 5 + 30).clamp(10, 40);
    let header = Row::new(["KIND", "REGIONS", "VIRTUAL", "RSS", "PSS", "SWAP", "DIRTY", "SHARE OF RSS", "MEANING"])
        .style(Style::new().fg(TITLE).add_modifier(Modifier::BOLD));
    let rows: Vec<Row> = stats
        .iter()
        .map(|s| {
            let share = ratio(s.rss, total_rss);
            let filled = (share * bar_w as f64).round() as usize;
            Row::new(vec![
                Cell::from(Line::from(vec![
                    Span::styled("■ ", Style::new().fg(s.kind.color())),
                    Span::styled(s.kind.label(), Style::new().fg(s.kind.color())),
                ])),
                Cell::from(s.count.to_string()),
                Cell::from(fmt_bytes(s.size)),
                Cell::from(fmt_bytes(s.rss)),
                Cell::from(fmt_bytes(s.pss)),
                Cell::from(fmt_bytes(s.swap)),
                Cell::from(fmt_bytes(s.dirty)),
                Cell::from(Line::from(vec![
                    Span::styled("■".repeat(filled), Style::new().fg(s.kind.color())),
                    Span::styled("■".repeat(bar_w - filled.min(bar_w)), Style::new().fg(TRACK)),
                    Span::styled(format!("{:>6.1}%", share * 100.0), Style::new().fg(TEXT)),
                ])),
                Cell::from(s.kind.describe()).style(Style::new().fg(DIM)),
            ])
            .style(Style::new().fg(TEXT))
        })
        .collect();
    let widths = [
        Constraint::Length(11),
        Constraint::Length(8),
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Length(bar_w as u16 + 8),
        Constraint::Min(10),
    ];
    f.render_widget(Table::new(rows, widths).header(header).block(block), area);
}

fn perm_text(p: &str) -> String {
    let b = p.as_bytes();
    let mut parts = Vec::new();
    if b.first() == Some(&b'r') {
        parts.push("read");
    }
    if b.get(1) == Some(&b'w') {
        parts.push("write");
    }
    if b.get(2) == Some(&b'x') {
        parts.push("execute");
    }
    if parts.is_empty() {
        parts.push("no access");
    }
    let sharing = if b.get(3) == Some(&b's') { "shared" } else { "private (copy-on-write)" };
    format!("{}; {sharing}", parts.join(", "))
}

fn draw_details(f: &mut Frame, area: Rect, app: &App) {
    let Some(r) = app.selected_region() else { return };
    let rect = centered(area, 70, 80);
    f.render_widget(Clear, rect);
    let block = panel(format!("region {:x}-{:x}", r.start, r.end))
        .title_bottom(Line::from([vec![Span::styled("┤", Style::new().fg(BORDER))], hint("↑↓", "prev/next"), hint("any key", "close"), vec![Span::styled("├", Style::new().fg(BORDER))]].concat()));
    let kv = |k: &str, v: String, c: Color| {
        Line::from(vec![Span::styled(format!("{k:<14}"), Style::new().fg(DIM)), Span::styled(v, Style::new().fg(c))])
    };
    let mut lines = vec![
        kv("range", format!("0x{:x} - 0x{:x}", r.start, r.end), TITLE),
        kv("size", format!("{} ({} pages of 4 KiB)", fmt_bytes(r.size()), r.size() / 4096), TEXT),
        kv("permissions", format!("{}  →  {}", r.perms, perm_text(&r.perms)), TEXT),
        kv("kind", format!("{} — {}", r.kind.label(), r.kind.describe()), r.kind.color()),
        kv("path", if r.path.is_empty() { "(anonymous)".into() } else { r.path.clone() }, TEXT),
        kv("file offset", format!("0x{:x}", r.offset), TEXT),
        kv("device:inode", format!("{} : {}", r.dev, r.inode), TEXT),
        Line::default(),
    ];
    match &r.smaps {
        None => lines.push(Line::styled("smaps counters unavailable for this process", Style::new().fg(DIM))),
        Some(s) => {
            lines.push(Line::styled("/proc/PID/smaps", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)));
            let half = s.raw.len().div_ceil(2);
            let col_w = (rect.width as usize / 2).saturating_sub(2);
            for i in 0..half {
                let mut spans = Vec::new();
                for (k, v) in [s.raw.get(i), s.raw.get(i + half)].into_iter().flatten() {
                    let text = truncate(&format!("{k:<16}{v:>10}"), col_w);
                    spans.push(Span::styled(format!("{text:<col_w$}"), Style::new().fg(TEXT)));
                    spans.push(Span::raw("  "));
                }
                lines.push(Line::from(spans));
            }
        }
    }
    f.render_widget(Paragraph::new(lines).block(block), rect);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let rect = centered(area, 70, 85);
    f.render_widget(Clear, rect);
    let keys = [
        ("↑ ↓  j k", "move the selection (mouse wheel too)"),
        ("PgUp PgDn g G", "page / first / last"),
        ("Enter, click", "details of the selected region"),
        ("click the map", "select the region under the cursor"),
        ("s / r", "change sort column / reverse order"),
        ("f", "filter by region kind (cycles)"),
        ("t, Tab", "switch between region table and per-kind summary"),
        ("p", "pick another process (type to search)"),
        ("space", "pause / resume refreshing"),
        ("+ / -", "refresh slower / faster (0.2 s - 10 s)"),
        ("u", "refresh now"),
        ("q, Esc, Ctrl+C", "quit"),
    ];
    let mut lines: Vec<Line> = keys
        .iter()
        .map(|(k, t)| Line::from(vec![Span::styled(format!("{k:<18}"), Style::new().fg(ACCENT)), Span::styled(*t, Style::new().fg(TEXT))]))
        .collect();
    lines.push(Line::default());
    lines.push(Line::styled("Region kinds", Style::new().fg(TITLE).add_modifier(Modifier::BOLD)));
    for k in Kind::ALL {
        lines.push(Line::from(vec![
            Span::styled(format!("■ {:<11}", k.label()), Style::new().fg(k.color())),
            Span::styled(k.describe(), Style::new().fg(TEXT)),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::styled(
        "Data: /proc/PID/maps, smaps, status and /proc/meminfo (read-only).",
        Style::new().fg(DIM),
    ));
    f.render_widget(Paragraph::new(lines).block(panel("help · any key to close".into())), rect);
}

fn draw_picker(f: &mut Frame, area: Rect, p: &mut Picker, uid: u32) {
    let rect = centered(area, 80, 80);
    f.render_widget(Clear, rect);
    let block = panel("select a process · type to search · ⏎ open · esc cancel".into());
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let [search, list] = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(inner);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("search: ", Style::new().fg(DIM)),
                Span::styled(p.query.clone(), Style::new().fg(TITLE).add_modifier(Modifier::BOLD)),
                Span::styled("▏", Style::new().fg(ACCENT)),
            ]),
            Line::styled("dimmed rows belong to other users: their memory maps need root to read", Style::new().fg(DIM)),
        ]),
        search,
    );
    let rows: Vec<Row> = p
        .filtered()
        .into_iter()
        .map(|pr| {
            let mine = pr.uid == uid;
            let color = if mine { TEXT } else { DIM };
            Row::new(vec![
                Cell::from(pr.pid.to_string()),
                Cell::from(if mine { "you".to_string() } else { pr.uid.to_string() }),
                Cell::from(fmt_bytes(pr.rss)),
                Cell::from(pr.name.clone()).style(Style::new().fg(if mine { ACCENT } else { DIM })),
                Cell::from(pr.cmdline.clone()),
            ])
            .style(Style::new().fg(color))
        })
        .collect();
    let table = Table::new(
        rows,
        [Constraint::Length(8), Constraint::Length(6), Constraint::Length(8), Constraint::Length(16), Constraint::Min(10)],
    )
    .header(Row::new(["PID", "USER", "RSS", "NAME", "COMMAND"]).style(Style::new().fg(TITLE).add_modifier(Modifier::BOLD)))
    .row_highlight_style(Style::new().bg(SEL_BG).add_modifier(Modifier::BOLD))
    .highlight_symbol(Span::styled("▶", Style::new().fg(ACCENT)))
    .highlight_spacing(HighlightSpacing::Always);
    f.render_stateful_widget(table, list, &mut p.state);
}
