//! Console progress line and per-stage timing summary.

use std::io::Write as _;
use std::ops::ControlFlow;
use std::time::Instant;

use stlink::flash::{Progress, Stage};

/// Prints `Stage  NN% (done/total)` on one line per stage and remembers when
/// each stage started.
pub struct ProgressPrinter {
    started: Instant,
    last: Option<(Stage, u64)>,
    /// `(stage, started, total)`.
    stages: Vec<(Stage, Instant, u64)>,
}

impl ProgressPrinter {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            last: None,
            stages: Vec::new(),
        }
    }

    /// Progress callback for the library.
    pub fn update(&mut self, p: Progress) -> ControlFlow<()> {
        if self.stages.last().is_none_or(|s| s.0 != p.stage) {
            self.stages.push((p.stage, Instant::now(), p.total));
        }
        let pct = (p.done * 100).checked_div(p.total).unwrap_or(100);
        if self.last != Some((p.stage, pct)) {
            if self.last.is_some_and(|(stage, _)| stage != p.stage) {
                eprintln!();
            }
            self.last = Some((p.stage, pct));
            eprint!(
                "\r{:<12} {pct:3}% ({}/{})",
                stage_name(p.stage),
                p.done,
                p.total
            );
            let _ = std::io::stderr().flush();
        }
        ControlFlow::Continue(())
    }

    /// End the progress line.
    pub fn finish(&self) {
        if self.last.is_some() {
            eprintln!();
        }
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// `erase 0.93 s, programming 0.19 s (88.6 KiB/s), ...`
    pub fn summary(&self) -> String {
        let end = Instant::now();
        self.stages
            .iter()
            .enumerate()
            .map(|(i, &(stage, t0, total))| {
                let t1 = self.stages.get(i + 1).map_or(end, |s| s.1);
                let secs = (t1 - t0).as_secs_f64();
                match stage {
                    // Erase progress counts sectors, not bytes.
                    Stage::Erasing => format!("erase {secs:.2} s"),
                    _ => format!(
                        "{} {secs:.2} s ({})",
                        stage_name(stage).to_lowercase(),
                        rate(total, secs)
                    ),
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Erasing => "Erasing",
        Stage::Programming => "Programming",
        Stage::Verifying => "Verifying",
        Stage::Reading => "Reading",
    }
}

/// `123.4 KiB/s`
pub fn rate(bytes: u64, secs: f64) -> String {
    format!("{:.1} KiB/s", bytes as f64 / 1024.0 / secs.max(1e-9))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_format() {
        assert_eq!(rate(2048, 2.0), "1.0 KiB/s");
        assert_eq!(rate(0, 0.0), "0.0 KiB/s");
    }

    #[test]
    fn summary_lists_stages_in_order() {
        let mut p = ProgressPrinter::new();
        for stage in [Stage::Erasing, Stage::Programming, Stage::Verifying] {
            let _ = p.update(Progress {
                stage,
                done: 0,
                total: 1024,
            });
        }
        let s = p.summary();
        let names: Vec<&str> = s
            .split(", ")
            .map(|x| x.split(' ').next().unwrap_or(""))
            .collect();
        assert_eq!(names, ["erase", "programming", "verifying"]);
    }
}
