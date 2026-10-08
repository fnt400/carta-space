//! Optional, content-free timing diagnostics.
//! Enable with CARTA_GUI_PROFILE=1. Nothing is logged by default.
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Counts {
    count: u64,
    total: Duration,
    worst: Duration,
}
impl Counts {
    fn add(&mut self, duration: Duration) {
        self.count += 1;
        self.total += duration;
        self.worst = self.worst.max(duration);
    }
    fn average_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total.as_secs_f64() * 1000.0 / self.count as f64
        }
    }
    fn worst_ms(&self) -> f64 {
        self.worst.as_secs_f64() * 1000.0
    }
}
struct Profile {
    since: Instant,
    view: Counts,
    scroll: Counts,
    input: Counts,
    maintenance: Counts,
}
impl Profile {
    fn new() -> Self {
        Self {
            since: Instant::now(),
            view: Counts::default(),
            scroll: Counts::default(),
            input: Counts::default(),
            maintenance: Counts::default(),
        }
    }
}
static PROFILER: OnceLock<Option<Mutex<Profile>>> = OnceLock::new();

pub fn enabled() -> bool {
    PROFILER
        .get_or_init(|| {
            (std::env::var("CARTA_GUI_PROFILE").as_deref() == Ok("1"))
                .then(|| Mutex::new(Profile::new()))
        })
        .is_some()
}

pub fn record(which: &str, started: Instant) {
    let Some(profile) = PROFILER.get_or_init(|| {
        (std::env::var("CARTA_GUI_PROFILE").as_deref() == Ok("1"))
            .then(|| Mutex::new(Profile::new()))
    }) else {
        return;
    };
    let elapsed = started.elapsed();
    let Ok(mut profile) = profile.lock() else {
        return;
    };
    match which {
        "view" => profile.view.add(elapsed),
        "scroll" => profile.scroll.add(elapsed),
        "input" => profile.input.add(elapsed),
        "maintenance" => profile.maintenance.add(elapsed),
        _ => return,
    }
    if profile.since.elapsed() >= Duration::from_secs(15) {
        eprintln!(
            "carta-gui profile: input {} avg {:.2} ms max {:.2} ms; view {} avg {:.2} ms max {:.2} ms; scroll {} avg {:.2} ms max {:.2} ms; maintenance {} avg {:.2} ms max {:.2} ms",
            profile.input.count, profile.input.average_ms(), profile.input.worst_ms(),
            profile.view.count, profile.view.average_ms(), profile.view.worst_ms(),
            profile.scroll.count, profile.scroll.average_ms(), profile.scroll.worst_ms(),
            profile.maintenance.count, profile.maintenance.average_ms(), profile.maintenance.worst_ms(),
        );
        *profile = Profile::new();
    }
}

#[cfg(test)]
mod tests {
    use super::Counts;
    use std::time::Duration;
    #[test]
    fn averaging_an_empty_sample_is_safe() {
        let mut counts = Counts::default();
        assert_eq!(counts.average_ms(), 0.0);
        counts.add(Duration::from_millis(20));
        counts.add(Duration::from_millis(40));
        assert_eq!(counts.average_ms(), 30.0);
        assert_eq!(counts.worst_ms(), 40.0);
    }
}
