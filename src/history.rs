use std::collections::VecDeque;

pub const CAPACITY: usize = 43_201;
pub const RANGES: [u64; 5] = [60, 600, 3600, 21600, 43200];
pub fn range_labels() -> [&'static str; 5] {
    ["1分", "10分", "60分", "6時間", "12時間"].map(crate::i18n::tr)
}
pub fn names() -> [&'static str; 6] {
    [
        "CPU",
        crate::i18n::tr("メモリ"),
        crate::i18n::tr("ディスク"),
        "Ethernet",
        "Wi-Fi",
        "GPU",
    ]
}

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
    #[allow(dead_code)] // Convenience wrapper for reference tests and standalone profiling.
    pub fn graph(&self, end: f64, seconds: u64, width: usize, visible: &[bool; 6]) -> Graph {
        let mut graph = Graph::default();
        graph.reset_buckets(width);
        let maximum = self.accumulate_for_display(end, seconds, &mut graph.buckets, visible);
        graph.finish_network(visible, maximum);
        graph
    }
    pub(crate) fn accumulate(&self, end: f64, seconds: u64, buckets: &mut [[Bucket; 6]]) {
        self.accumulate_impl::<false>(end, seconds, buckets, &[false; 6]);
    }
    pub(crate) fn accumulate_for_display(
        &self,
        end: f64,
        seconds: u64,
        buckets: &mut [[Bucket; 6]],
        visible: &[bool; 6],
    ) -> Option<f32> {
        // Sparse timelines have fewer samples than columns. Avoid scanning a
        // wide empty bucket array just to find the network maximum.
        if seconds <= buckets.len() as u64 {
            Some(self.accumulate_impl::<true>(end, seconds, buckets, visible))
        } else {
            self.accumulate(end, seconds, buckets);
            None
        }
    }
    fn accumulate_impl<const INLINE_NETWORK: bool>(
        &self,
        end: f64,
        seconds: u64,
        buckets: &mut [[Bucket; 6]],
        visible: &[bool; 6],
    ) -> f32 {
        let width = buckets.len();
        let start = end - seconds as f64;
        let mut maximum = 0.1_f32;
        for sample in self.samples.iter().rev() {
            if sample.time < start {
                break;
            }
            if sample.time > end {
                continue;
            }
            let x = (((sample.time - start) / seconds as f64 * (width - 1) as f64) as usize)
                .min(width - 1);
            for (b, &v) in buckets[x].iter_mut().zip(&sample.values) {
                if !v.is_finite() {
                    continue;
                }
                b.min = b.min.min(v);
                b.max = b.max.max(v);
                b.sum += v;
                b.count += 1;
                b.first_time = b.first_time.min(sample.time);
                b.last_time = b.last_time.max(sample.time);
            }
            if INLINE_NETWORK {
                for series in [3, 4] {
                    let value = sample.values[series];
                    if visible[series] && value.is_finite() {
                        maximum = maximum.max(value);
                    }
                }
            }
        }
        maximum
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
#[derive(Default)]
pub struct Graph {
    pub buckets: Vec<[Bucket; 6]>,
    pub network_top: f32,
}
impl Graph {
    pub(crate) fn finish_network(&mut self, visible: &[bool; 6], maximum: Option<f32>) {
        if let Some(maximum) = maximum {
            self.network_top = nice_ceiling(maximum * 1.1);
        } else {
            self.update_network_top(visible);
        }
    }
    pub(crate) fn reset_buckets(&mut self, width: usize) {
        self.buckets.clear();
        self.buckets
            .resize(width.clamp(2, 4096), [Bucket::default(); 6]);
    }
    pub(crate) fn update_network_top(&mut self, visible: &[bool; 6]) {
        let mut maximum = 0.1_f32;
        if visible[3] || visible[4] {
            for column in &self.buckets {
                if visible[3] {
                    maximum = maximum.max(column[3].max);
                }
                if visible[4] {
                    maximum = maximum.max(column[4].max);
                }
            }
        }
        self.network_top = nice_ceiling(maximum * 1.1);
    }
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

#[cfg(test)]
mod optimization_tests {
    use super::*;
    fn reference_graph(
        history: &History,
        end: f64,
        seconds: u64,
        width: usize,
        visible: &[bool; 6],
    ) -> Graph {
        let width = width.clamp(2, 4096);
        let mut buckets = vec![[Bucket::default(); 6]; width];
        let start = end - seconds as f64;
        let mut max_mbps = 0.1_f32;
        for sample in history.samples.iter().rev() {
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

    fn assert_same(expected: &Graph, actual: &Graph) {
        assert_eq!(expected.network_top.to_bits(), actual.network_top.to_bits());
        assert_eq!(expected.buckets.len(), actual.buckets.len());
        for (a, b) in expected
            .buckets
            .iter()
            .flatten()
            .zip(actual.buckets.iter().flatten())
        {
            assert_eq!(a.count, b.count);
            assert_eq!(a.min.to_bits(), b.min.to_bits());
            assert_eq!(a.max.to_bits(), b.max.to_bits());
            assert_eq!(a.sum.to_bits(), b.sum.to_bits());
            assert_eq!(a.first_time.to_bits(), b.first_time.to_bits());
            assert_eq!(a.last_time.to_bits(), b.last_time.to_bits());
        }
    }
    #[test]
    fn reused_graph_matches_original_for_ranges_gaps_wrap_and_visibility() {
        let mut history = History::new();
        for i in 0..CAPACITY + 73 {
            history.push(Sample {
                time: i as f64 + (i / 97) as f64 * 3.,
                values: std::array::from_fn(|j| match (i + j) % 31 {
                    0 => f32::NAN,
                    1 => f32::INFINITY,
                    2 => f32::NEG_INFINITY,
                    _ => ((i * (j + 3)) % 10001) as f32 / 100. - 1.,
                }),
                ..Sample::default()
            });
        }
        let mut graph = Graph::default();
        for seconds in RANGES {
            for width in [0, 1, 2, 280, 960, 1920, 4096, 4097] {
                for mask in 0..4 {
                    let mut visible = [true; 6];
                    visible[3] = mask & 1 != 0;
                    visible[4] = mask & 2 != 0;
                    let end = history.latest().time - 0.5;
                    graph.reset_buckets(width);
                    let maximum =
                        history.accumulate_for_display(end, seconds, &mut graph.buckets, &visible);
                    graph.finish_network(&visible, maximum);
                    assert_same(
                        &reference_graph(&history, end, seconds, width, &visible),
                        &graph,
                    );
                }
            }
        }
        let empty = History::new();
        for width in [1920, 280, 960, 4096, 2] {
            graph.reset_buckets(width);
            empty.accumulate(100., 60, &mut graph.buckets);
            graph.update_network_top(&[true; 6]);
            assert_same(
                &reference_graph(&empty, 100., 60, width, &[true; 6]),
                &graph,
            );
        }
        let pointer = graph.buckets.as_ptr();
        for _ in 0..10 {
            graph.reset_buckets(2);
        }
        assert_eq!(pointer, graph.buckets.as_ptr());
    }
    #[test]
    fn reused_graph_matches_original_at_boundaries_and_peaks() {
        let mut history = History::new();
        for (time, value) in [
            (-0.001, 91.),
            (0., 100.),
            (2.49, 3.),
            (4.99, 4.),
            (7.5, 5.),
            (30., f32::NAN),
            (60., 75.),
            (60., 90.),
            (60.001, 999.),
        ] {
            history.push(Sample {
                time,
                values: [value; 6],
                ..Sample::default()
            });
        }
        for end in [59.999, 60., 60.001] {
            for width in [2, 280, 960] {
                assert_same(
                    &reference_graph(&history, end, 60, width, &[true; 6]),
                    &history.graph(end, 60, width, &[true; 6]),
                );
            }
        }
    }
}
