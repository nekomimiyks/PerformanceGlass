use std::collections::VecDeque;

pub const CAPACITY: usize = 43_201;
pub const RANGES: [u64; 5] = [60, 600, 3600, 21600, 43200];
pub const RANGE_LABELS: [&str; 5] = ["1分", "10分", "60分", "6時間", "12時間"];
pub const NAMES: [&str; 6] = ["CPU", "メモリ", "ディスク", "Ethernet", "Wi-Fi", "GPU"];

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub time: f64,
    // CPU, physical memory, busiest disk; network Mbps (RX + TX); busiest GPU engine.
    // NaN means unavailable, never an artificial zero.
    pub values: [f32; 6],
    pub ghz: f32,
    pub memory_used: f32,
    pub memory_total: f32,
    /// System-wide dedicated GPU memory usage, GiB. Not per-process sums.
    pub gpu_memory: f32,
    /// All physical disks combined, decimal MB/s: read, write.
    pub disk_io: [f32; 2],
    /// CPU package / hottest GPU, degrees Celsius. NaN means no real sensor.
    pub temperatures: [f32; 2],
    pub traffic: [[f32; 2]; 2],
    pub connected: [bool; 2],
}
impl Default for Sample {
    fn default() -> Self {
        Self {
            time: 0.,
            values: [f32::NAN; 6],
            ghz: f32::NAN,
            memory_used: 0.,
            memory_total: 0.,
            gpu_memory: f32::NAN,
            disk_io: [f32::NAN; 2],
            temperatures: [f32::NAN; 2],
            traffic: [[f32::NAN; 2]; 2],
            connected: [false; 2],
        }
    }
}

pub struct History {
    pub samples: VecDeque<Sample>,
}
impl History {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(CAPACITY),
        }
    }
    pub fn push(&mut self, sample: Sample) {
        if self.samples.len() == CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }
    pub fn latest(&self) -> Sample {
        self.samples.back().copied().unwrap_or_default()
    }
    pub fn nearest(&self, time: f64) -> Option<Sample> {
        // Binary search over logical VecDeque indices, including after wraparound.
        let mut lo = 0;
        let mut hi = self.samples.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.samples[mid].time < time {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let a = self.samples.get(lo).copied();
        let b = lo.checked_sub(1).and_then(|i| self.samples.get(i)).copied();
        match (a, b) {
            (Some(a), Some(b)) => Some(if (a.time - time).abs() < (b.time - time).abs() {
                a
            } else {
                b
            }),
            (a, b) => a.or(b),
        }
    }
    pub fn graph(&self, end: f64, seconds: u64, width: usize, visible: &[bool; 6]) -> Graph {
        let width = width.clamp(2, 4096);
        let mut buckets = vec![[Bucket::default(); 6]; width];
        let start = end - seconds as f64;
        let mut max_mbps = 0.1_f32;
        for sample in self.samples.iter().rev() {
            if sample.time < start {
                break;
            }
            if sample.time > end {
                continue;
            }
            let x = (((sample.time - start) / seconds as f64 * (width - 1) as f64) as usize)
                .min(width - 1);
            for (series, &v) in sample.values.iter().enumerate() {
                if !v.is_finite() {
                    continue;
                }
                let b = &mut buckets[x][series];
                b.min = b.min.min(v);
                b.max = b.max.max(v);
                b.sum += v;
                b.count += 1;
                b.first_time = b.first_time.min(sample.time);
                b.last_time = b.last_time.max(sample.time);
                if (series == 3 || series == 4) && visible[series] {
                    max_mbps = max_mbps.max(v);
                }
            }
        }
        Graph {
            buckets,
            network_top: nice_ceiling(max_mbps * 1.1),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Bucket {
    pub min: f32,
    pub max: f32,
    pub sum: f32,
    pub count: u32,
    pub first_time: f64,
    pub last_time: f64,
}
impl Default for Bucket {
    fn default() -> Self {
        Self {
            min: f32::INFINITY,
            max: f32::NEG_INFINITY,
            sum: 0.,
            count: 0,
            first_time: f64::INFINITY,
            last_time: f64::NEG_INFINITY,
        }
    }
}
pub struct Graph {
    pub buckets: Vec<[Bucket; 6]>,
    pub network_top: f32,
}
pub fn nice_ceiling(v: f32) -> f32 {
    let base = 10_f32.powf(v.max(0.001).log10().floor());
    [1., 2., 5., 10.]
        .into_iter()
        .find(|n| v <= n * base)
        .unwrap_or(10.)
        * base
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_and_nearest_after_wrap() {
        let mut h = History::new();
        for i in 0..CAPACITY + 20 {
            h.push(Sample {
                time: i as f64,
                ..Sample::default()
            });
        }
        assert_eq!(h.samples.len(), CAPACITY);
        assert_eq!(h.samples.front().unwrap().time, 20.);
        assert_eq!(h.nearest(35.2).unwrap().time, 35.);
        assert_eq!(h.nearest(-1.).unwrap().time, 20.);
    }
    #[test]
    fn peaks_missing_values_and_real_time_gaps() {
        let mut h = History::new();
        for (t, v) in [(0., 0.), (1., 100.), (2., 0.), (60., 30.)] {
            h.push(Sample {
                time: t,
                values: [v, f32::NAN, 0., 4., 0., 0.],
                ..Sample::default()
            });
        }
        let g = h.graph(60., 60, 10, &[true; 6]);
        assert_eq!(g.buckets[0][0].max, 100.);
        assert_eq!(g.buckets[0][1].count, 0);
        assert_eq!(g.buckets[5][0].count, 0);
        assert_eq!(g.network_top, 5.);
    }
}
