//! Flight recorder: what flux was doing when it crashed or hung.
//!
//! Always on (costs a short string per message):
//! - a trail of the last messages the UI handled;
//! - a panic hook that writes the panic, a backtrace and that trail to
//!   `~/.cache/flux/crash-<time>.log`.
//!
//! With `FLUX_DEBUG=1`, also:
//! - a watchdog thread: when the UI thread stops responding for more than
//!   [`HANG_AFTER`], it writes `hang-<time>.log` with the message being handled, the trail
//!   and every thread's stack (via `gdb` or `eu-stack`), and logs when it recovers;
//! - `[slow AppMsg]` for any message that takes more than 32 ms.

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::io::{Read as _, Seek as _, Write as _};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const TRAIL_LEN: usize = 60;
const NAME_LEN: usize = 100;
const HANG_AFTER: Duration = Duration::from_secs(2);
const HEARTBEAT: Duration = Duration::from_millis(250);
const SLOW_MSG: Duration = Duration::from_millis(32);

struct Entry {
    at: Instant,
    name: String,
    took: Duration,
}

static START: OnceLock<Instant> = OnceLock::new();
static TRAIL: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());
/// The message being handled right now (and since when).
static CURRENT: Mutex<Option<(Instant, String)>> = Mutex::new(None);
/// Milliseconds since START at the UI thread's last heartbeat.
static BEAT: AtomicU64 = AtomicU64::new(0);
static DEBUG: AtomicBool = AtomicBool::new(false);

fn start() -> Instant {
    *START.get_or_init(Instant::now)
}

fn since_start(at: Instant) -> f64 {
    at.saturating_duration_since(start()).as_secs_f64()
}

/// Whether `FLUX_DEBUG` is set.
pub fn debug() -> bool {
    DEBUG.load(Ordering::Relaxed)
}

/// Installs the panic hook, and with `FLUX_DEBUG` the watchdog. Call on the main thread
/// before the GTK main loop starts (the heartbeat runs on its default context).
pub fn install() {
    start();
    let debug = std::env::var_os("FLUX_DEBUG").is_some_and(|v| v != "0");
    DEBUG.store(debug, Ordering::Relaxed);

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut report = header("crash");
        let thread = std::thread::current();
        let _ = writeln!(
            report,
            "panic on thread '{}': {info}\n",
            thread.name().unwrap_or("<unnamed>")
        );
        let _ = writeln!(
            report,
            "backtrace:\n{}\n",
            std::backtrace::Backtrace::force_capture()
        );
        write_state(&mut report);
        if let Some(path) = save("crash", &report) {
            eprintln!("[flux] crash report written to {}", path.display());
        }
        default_hook(info);
    }));

    if debug {
        eprintln!(
            "[flux] FLUX_DEBUG: crash/hang reports go to {}",
            dir().display()
        );
        allow_self_trace();
        gtk::glib::timeout_add_local(HEARTBEAT, || {
            BEAT.store(start().elapsed().as_millis() as u64, Ordering::Relaxed);
            gtk::glib::ControlFlow::Continue
        });
        let _ = std::thread::Builder::new()
            .name("flux-watchdog".into())
            .spawn(watchdog);
    }
}

/// Records `message` as being handled until the returned guard drops. `describe` gives
/// its debug text; only the first [`NAME_LEN`] characters are kept (and formatted).
pub fn message(describe: impl std::fmt::Debug) -> MessageGuard {
    let mut name = Truncated(String::new());
    let _ = write!(name, "{describe:?}");
    let at = Instant::now();
    *CURRENT.lock() = Some((at, name.0.clone()));
    MessageGuard { at, name: name.0 }
}

pub struct MessageGuard {
    at: Instant,
    name: String,
}

impl Drop for MessageGuard {
    fn drop(&mut self) {
        let took = self.at.elapsed();
        *CURRENT.lock() = None;
        if took > SLOW_MSG && debug() {
            eprintln!("[slow AppMsg] {took:?} {}", self.name);
        }
        let mut trail = TRAIL.lock();
        if trail.len() == TRAIL_LEN {
            trail.pop_front();
        }
        trail.push_back(Entry {
            at: self.at,
            name: std::mem::take(&mut self.name),
            took,
        });
    }
}

/// A `fmt::Write` that stops (cheaply) after [`NAME_LEN`] characters.
struct Truncated(String);

impl std::fmt::Write for Truncated {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let room = NAME_LEN.saturating_sub(self.0.chars().count());
        self.0.extend(s.chars().take(room));
        if room <= s.chars().count() {
            Err(std::fmt::Error)
        } else {
            Ok(())
        }
    }
}

fn watchdog() {
    let mut reported = false;
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let now = start().elapsed().as_millis() as u64;
        let last = BEAT.load(Ordering::Relaxed);
        if last == 0 {
            continue; // main loop not running yet
        }
        let stalled = Duration::from_millis(now.saturating_sub(last));
        if stalled > HANG_AFTER && !reported {
            reported = true;
            let mut report = header("hang");
            let _ = writeln!(report, "UI thread unresponsive for {stalled:?}\n");
            write_state(&mut report);
            let _ = writeln!(report, "\nthreads:\n{}", thread_stacks());
            match save("hang", &report) {
                Some(path) => eprintln!(
                    "[flux] UI thread stuck for {stalled:?}; report in {}",
                    path.display()
                ),
                None => eprintln!("[flux] UI thread stuck for {stalled:?}\n{report}"),
            }
        } else if stalled < HEARTBEAT * 2 && reported {
            reported = false;
            eprintln!("[flux] UI thread responsive again");
        }
    }
}

fn header(kind: &str) -> String {
    format!(
        "flux {kind} report\nversion {} ({})\ntime {}\nuptime {:.1}s\n\n",
        env!("CARGO_PKG_VERSION"),
        option_env!("FLUX_GIT_HASH").unwrap_or("?"),
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        start().elapsed().as_secs_f64(),
    )
}

/// The message in progress and the trail. Uses `try_lock` so a report can still be
/// written if the hang or panic happened while one of these was held.
fn write_state(report: &mut String) {
    match CURRENT.try_lock().as_deref() {
        Some(Some((at, name))) => {
            let _ = writeln!(
                report,
                "handling: {name}\n  for {:?} (since {:.3}s)\n",
                at.elapsed(),
                since_start(*at)
            );
        }
        Some(None) => {
            let _ = writeln!(report, "handling: (no message in progress)\n");
        }
        None => {
            let _ = writeln!(report, "handling: (unknown, lock busy)\n");
        }
    }
    let _ = writeln!(report, "last messages (oldest first):");
    match TRAIL.try_lock() {
        Some(trail) => {
            for e in trail.iter() {
                let _ = writeln!(
                    report,
                    "  {:>9.3}s {} [{:?}]",
                    since_start(e.at),
                    e.name,
                    e.took
                );
            }
        }
        None => {
            let _ = writeln!(report, "  (unavailable, lock busy)");
        }
    }
}

/// Every thread's native stack, from `gdb` (demangled, with source lines) or `eu-stack`,
/// whichever is installed.
fn thread_stacks() -> String {
    let pid = std::process::id().to_string();
    let tries: [(&str, Vec<&str>); 2] = [
        (
            "gdb",
            vec!["-p", &pid, "-batch", "-ex", "thread apply all bt"],
        ),
        ("eu-stack", vec!["-p", &pid]),
    ];
    for (tool, args) in tries {
        // The debugger stops every thread, this one included, so its output goes to a
        // file: a pipe would fill up with nobody left to drain it, deadlocking both.
        let Ok(mut file) = tempfile::tempfile() else {
            continue;
        };
        let Ok(stdout) = file.try_clone() else {
            continue;
        };
        let status = std::process::Command::new(tool)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(stdout)
            .stderr(std::process::Stdio::null())
            .status();
        let mut out = String::new();
        if status.is_ok_and(|s| s.success())
            && file.seek(std::io::SeekFrom::Start(0)).is_ok()
            && file.read_to_string(&mut out).is_ok()
            && !out.is_empty()
        {
            return format!("({tool})\n{out}");
        }
    }
    "(install gdb or elfutils (eu-stack) for thread stacks)".into()
}

/// Lets a debugger that flux starts itself attach to it, despite Yama's
/// `ptrace_scope = 1` (only ancestors may trace by default).
fn allow_self_trace() {
    // SAFETY: plain prctl call with integer arguments.
    unsafe {
        libc::prctl(libc::PR_SET_PTRACER, libc::PR_SET_PTRACER_ANY, 0, 0, 0);
    }
}

fn dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("flux")
}

fn save(kind: &str, report: &str) -> Option<PathBuf> {
    let dir = dir();
    std::fs::create_dir_all(&dir).ok()?;
    let name = format!(
        "{kind}-{}.log",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).ok()?;
    file.write_all(report.as_bytes()).ok()?;
    Some(path)
}
