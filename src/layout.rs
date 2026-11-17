use windows_sys::Win32::Foundation::RECT;

pub struct Layout {
    pub toolbar: bool,
    pub stats: bool,
    pub axes: bool,
    pub vertical_axes: bool,
    pub compact_axes: bool,
    pub plot: RECT,
    pub stats_y: i32,
}
impl Layout {
    pub fn new(w: i32, h: i32, scale: f64, focus: bool, stats_fit: bool, network: bool) -> Self {
        let px = |v: f64| (v * scale).round() as i32;
        let axes = !focus && h >= px(90.) && w >= px(220.);
        let vertical_axes = axes || focus;
        let compact_axes = focus && h < px(90.);
        let toolbar = axes && h >= px(140.);
        let stats = axes && h >= px(220.) && stats_fit;
        let stats_y = h - if stats { px(65.) } else { 0 };
        let plot = if focus {
            // Retain vertical scales even in a 60 physical pixel strip. Cap
            // the compact font/margins instead of scaling away the whole plot.
            let font = (11. * scale).round().min(12.) as i32;
            RECT {
                left: if compact_axes { 29 } else { px(36.) },
                top: if compact_axes { font / 2 + 2 } else { px(10.) },
                right: w - if network {
                    if compact_axes {
                        39
                    } else {
                        px(58.)
                    }
                } else {
                    4
                },
                bottom: h - if compact_axes { font / 2 + 2 } else { px(10.) },
            }
        } else if !axes {
            let margin = px(2.).min((w.min(h) - 1).max(0) / 2);
            RECT {
                left: margin,
                top: margin,
                right: w - margin,
                bottom: h - margin,
            }
        } else {
            RECT {
                left: px(36.),
                top: if toolbar { px(49.) } else { px(17.) },
                right: w - if network { px(58.) } else { px(9.) },
                bottom: stats_y - px(25.),
            }
        };
        Self {
            toolbar,
            stats,
            axes,
            vertical_axes,
            compact_axes,
            plot,
            stats_y,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tiny_physical_height_at_all_dpis() {
        for scale in [1., 1.25, 1.5, 2., 2.5] {
            let l = Layout::new(280, 60, scale, true, true, true);
            assert!(!l.toolbar && !l.stats && !l.axes);
            assert!(l.vertical_axes && l.compact_axes);
            assert!(l.plot.right > l.plot.left && l.plot.bottom > l.plot.top);
            assert!(l.plot.bottom <= 60 && l.plot.right <= 280);
        }
    }
    #[test]
    fn width_and_height_breakpoints_keep_plot_clear() {
        for (w, h) in [
            (960, 430),
            (680, 400),
            (420, 400),
            (280, 400),
            (280, 120),
            (140, 24),
            (140, 400),
        ] {
            let l = Layout::new(w, h, 1., false, w >= 620, true);
            assert!(
                l.plot.right > l.plot.left && l.plot.bottom > l.plot.top,
                "{w}x{h}"
            );
            assert!(!l.stats || l.plot.bottom < l.stats_y);
            assert!(!l.toolbar || l.plot.top >= 30);
            assert!(l.axes || (!l.toolbar && !l.stats));
        }
        let high_dpi = Layout::new(280, 400, 2., false, false, true);
        assert!(!high_dpi.toolbar && !high_dpi.stats && !high_dpi.axes);
    }
}
