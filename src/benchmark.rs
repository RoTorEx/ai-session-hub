//! Explicit diagnostic command; no Jump unless an exact session ID is supplied.
use crate::{activity, content, iterm, model, process};
use anyhow::{Result, bail};
use std::time::Instant;
fn measure(mut f: impl FnMut() -> Result<()>, samples: usize) -> Result<(f64, f64, f64, f64)> {
    let mut wall = vec![];
    let before = cpu();
    for _ in 0..samples {
        let t = Instant::now();
        f()?;
        wall.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let after = cpu();
    let used = after.0 - before.0;
    let terminal_used = after.1 - before.1;
    wall.sort_by(f64::total_cmp);
    Ok((
        wall[wall.len() / 2],
        wall[(wall.len() * 95 / 100).min(wall.len() - 1)],
        used * 1000.0 / samples as f64,
        terminal_used * 1000.0 / samples as f64,
    ))
}
// proc_taskinfo reports Mach ticks, not nanoseconds (notably on Apple Silicon).
#[repr(C)]
struct Timebase {
    numer: u32,
    denom: u32,
}
unsafe extern "C" {
    fn mach_timebase_info(info: *mut Timebase) -> i32;
}
fn task_cpu(pid: u32) -> f64 {
    let mut info = unsafe { std::mem::zeroed::<libc::proc_taskinfo>() };
    let size = std::mem::size_of::<libc::proc_taskinfo>();
    if unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDTASKINFO,
            0,
            (&mut info as *mut libc::proc_taskinfo).cast(),
            size as i32,
        )
    } != size as i32
    {
        return 0.0;
    }
    let mut base = Timebase { numer: 0, denom: 0 };
    if unsafe { mach_timebase_info(&mut base) } != 0 || base.denom == 0 {
        return 0.0;
    }
    (info.pti_total_user + info.pti_total_system) as f64 * base.numer as f64
        / base.denom as f64
        / 1e9
}
fn terminal_pid() -> u32 {
    static PID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *PID.get_or_init(|| {
        for p in process::scan().unwrap_or_default() {
            let mut bytes = [0i8; 4096];
            if unsafe {
                libc::proc_pidpath(p.pid as i32, bytes.as_mut_ptr().cast(), bytes.len() as u32)
            } > 0
            {
                let path = unsafe { std::ffi::CStr::from_ptr(bytes.as_ptr()) }.to_string_lossy();
                if path.ends_with("/iTerm2") {
                    return p.pid;
                }
            }
        }
        0
    })
}
fn cpu() -> (f64, f64) {
    let mut seconds = 0.0;
    for who in [libc::RUSAGE_SELF, libc::RUSAGE_CHILDREN] {
        let mut r = unsafe { std::mem::zeroed::<libc::rusage>() };
        if unsafe { libc::getrusage(who, &mut r) } == 0 {
            seconds += r.ru_utime.tv_sec as f64
                + r.ru_stime.tv_sec as f64
                + (r.ru_utime.tv_usec + r.ru_stime.tv_usec) as f64 / 1e6;
        }
    }
    for p in process::scan()
        .unwrap_or_default()
        .iter()
        .filter(|p| p.parent == std::process::id())
    {
        seconds += task_cpu(p.pid);
    }
    (seconds, task_cpu(terminal_pid()))
}
pub fn run(samples: usize, jump: Option<&str>) -> Result<()> {
    if !(1..=100).contains(&samples) {
        bail!("samples must be between 1 and 100");
    }
    let mut collector = model::Collector::new(3600);
    let sessions = collector.scan(true)?;
    println!(
        "AI Session Hub {} · {} live sessions · {} samples",
        env!("CARGO_PKG_VERSION"),
        sessions.len(),
        samples
    );
    println!(
        "stage\tmedian_ms\tp95_ms\tcpu_ms_per_scan (AI Session Hub + owned helpers)\titerm_cpu_ms (includes other terminal activity)"
    );
    let row = |name: &str, f: &mut dyn FnMut() -> Result<()>| -> Result<(f64, f64, f64, f64)> {
        let v = measure(f, samples)?;
        println!("{name}\t{:.2}\t{:.2}\t{:.2}\t{:.2}", v.0, v.1, v.2, v.3);
        Ok(v)
    };
    row("native processes", &mut || {
        process::scan()?;
        Ok(())
    })?;
    let fast = row("cached inventory", &mut || {
        collector.scan(false)?;
        Ok(())
    })?;
    let mut terminal = iterm::Client::default();
    terminal.warm()?;
    row("all session status", &mut || {
        activity::read(&sessions, &mut terminal)?;
        Ok(())
    })?;
    let meta = row("iTerm metadata", &mut || {
        terminal.panes()?;
        Ok(())
    })?;
    row("full collection", &mut || {
        model::collect()?;
        Ok(())
    })?;
    let selected = match jump {
        Some(id) => Some(
            sessions
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?,
        ),
        None => sessions.iter().find(|s| s.location.is_some()),
    };
    if let Some(s) = selected {
        let context = row("selected content", &mut || {
            content::read(s, &mut terminal)?;
            Ok(())
        })?;
        println!(
            "interval_s\tinventory_cpu_pct\tcontent_cpu_pct\tmetadata_cpu_pct (one core, estimate)"
        );
        for interval in [1, 2, 5, 10, 30] {
            println!(
                "{interval}\t{:.3}\t{:.3}\t{:.3}",
                fast.2 / (interval as f64 * 10.0),
                context.2 / (interval as f64 * 10.0),
                meta.2 / (interval as f64 * 10.0)
            );
        }
    }
    if let Some(id) = jump {
        let s = sessions
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| anyhow::anyhow!("Session not found"))?;
        row("exact Jump", &mut || model::jump_with(s, &mut terminal))?;
    }
    Ok(())
}
