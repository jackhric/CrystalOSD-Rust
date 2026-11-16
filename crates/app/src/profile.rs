use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Produce,
    Record,
    Present,
}

#[derive(Debug, Clone, Copy, Default)]
struct Stat {
    count: u64,
    sum: f64,
    worst: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Profile {
    screens: BTreeMap<String, [Stat; 3]>,
}

impl Profile {
    pub fn add(&mut self, screen: &str, stage: Stage, milliseconds: f64) {
        let stat = &mut self.screens.entry(screen.to_owned()).or_default()[stage as usize];
        stat.count += 1;
        stat.sum += milliseconds;
        if milliseconds > stat.worst {
            stat.worst = milliseconds;
        }
    }

    pub fn table(&self) -> String {
        let mut out = format!("{:<28} {:>7} {:>20} {:>20} {:>20}\n", "screen", "frames", "produce mean/worst", "record mean/worst", "present mean/worst");
        for (name, stats) in &self.screens {
            let mut row = format!("{:<28} {:>7}", if name.is_empty() { "(none)" } else { name }, stats[0].count);
            for s in stats {
                let mean = if s.count != 0 { s.sum / s.count as f64 } else { 0.0 };
                row += &format!(" {:>20}", format!("{mean:.3} / {:.3} ms", s.worst));
            }
            out += &row;
            out += "\n";
        }
        out
    }
}
