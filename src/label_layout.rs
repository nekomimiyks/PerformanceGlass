//! Each trace owns a fixed horizontal interval. Only its vertical position
//! follows the trace, so even identical values cannot make titles overlap.
use windows_sys::Win32::Foundation::RECT;

pub fn title_area(plot: RECT, scale: f64) -> RECT {
    let clear = ((plot.right - plot.left) / 6).clamp(40, (160. * scale).round() as i32);
    RECT {
        left: plot.left + 2,
        top: plot.top + 1,
        right: plot.right - clear,
        bottom: plot.bottom - 1,
    }
}
pub fn fits(area: RECT, widths: &[i32; 6]) -> bool {
    let gaps = widths.iter().filter(|&&w| w > 0).count().saturating_sub(1) as i32;
    widths.iter().sum::<i32>() + gaps * 2 <= area.right - area.left
}
pub fn horizontal_positions(area: RECT, widths: &[i32; 6]) -> [i32; 6] {
    let total = widths.iter().sum::<i32>();
    let free = (area.right - area.left - total).max(0);
    let gaps = widths.iter().filter(|&&w| w > 0).count().saturating_sub(1) as i32;
    // Pack the fixed slots immediately before the reserved latest-value area.
    // Extra window width must not spread the titles further apart.
    let gap = if gaps > 0 {
        (free / gaps).clamp(2, 4)
    } else {
        0
    };
    let start = area.right - total - gap * gaps;
    let mut used = 0;
    std::array::from_fn(|i| {
        let left = start + used;
        if widths[i] > 0 {
            used += widths[i] + gap;
        }
        left
    })
}
pub fn place(
    area: RECT,
    targets: &[(usize, i32)],
    widths: &[i32; 6],
    height: i32,
) -> Vec<(usize, RECT)> {
    let lefts = horizontal_positions(area, widths);
    targets
        .iter()
        .map(|&(series, y)| {
            let top = y.clamp(area.top, (area.bottom - height).max(area.top));
            (
                series,
                RECT {
                    left: lefts[series],
                    top,
                    right: lefts[series] + widths[series],
                    bottom: top + height,
                },
            )
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_hidden_subset_removes_both_width_and_extra_gaps() {
        let area = RECT {
            left: 40,
            top: 20,
            right: 800,
            bottom: 400,
        };
        let full_widths = [40, 50, 55, 70, 45, 40];
        for mask in 0..64 {
            let widths = std::array::from_fn(|i| {
                if mask & (1 << i) != 0 {
                    full_widths[i]
                } else {
                    0
                }
            });
            let lefts = horizontal_positions(area, &widths);
            let visible: Vec<_> = (0..6).filter(|&i| widths[i] > 0).collect();
            for pair in visible.windows(2) {
                assert_eq!(lefts[pair[1]] - (lefts[pair[0]] + widths[pair[0]]), 4);
            }
            if let Some(&last) = visible.last() {
                assert_eq!(lefts[last] + widths[last], area.right);
            }
        }
    }
    #[test]
    fn wide_windows_keep_titles_tightly_grouped() {
        let widths = [40, 50, 55, 70, 45, 40];
        for right in [800, 1600, 3200] {
            let area = RECT {
                left: 40,
                top: 20,
                right,
                bottom: 400,
            };
            let positions = horizontal_positions(area, &widths);
            for i in 1..6 {
                assert_eq!(positions[i] - (positions[i - 1] + widths[i - 1]), 4);
            }
            assert_eq!(positions[5] + widths[5], area.right);
        }
    }
    #[test]
    fn horizontal_intervals_never_overlap_even_at_identical_values() {
        for scale in [1., 1.5, 2., 2.5] {
            for (w, h) in [(280, 60), (680, 120), (960, 430)] {
                let plot = crate::layout::Layout::new(w, h, scale, true, false, true).plot;
                let area = title_area(plot, scale);
                let widths = [23, 25, 23, 22, 25, 24];
                assert!(fits(area, &widths));
                let targets: Vec<_> = (0..6).map(|i| (i, area.bottom)).collect();
                let titles = place(area, &targets, &widths, 14);
                for (i, (_, a)) in titles.iter().enumerate() {
                    assert!(a.left >= area.left && a.right <= area.right);
                    assert!(a.top >= area.top && a.bottom <= area.bottom);
                    for (_, b) in titles.iter().skip(i + 1) {
                        assert!(a.right < b.left);
                    }
                    assert!(a.right <= plot.right - 40);
                }
            }
        }
    }
    #[test]
    fn values_and_missing_traces_do_not_change_horizontal_positions() {
        let area = RECT {
            left: 40,
            top: 20,
            right: 800,
            bottom: 400,
        };
        let widths = [40, 50, 55, 70, 45, 40];
        let first = place(
            area,
            &[(0, 30), (1, 120), (2, 200), (3, 250), (4, 300), (5, 350)],
            &widths,
            20,
        );
        let next = place(area, &[(0, 320), (2, 320), (5, 30)], &widths, 20);
        for (series, b) in next {
            let a = first.iter().find(|(i, _)| *i == series).unwrap().1;
            assert_eq!(a.left, b.left);
            assert_eq!(a.right, b.right);
        }
    }
}
