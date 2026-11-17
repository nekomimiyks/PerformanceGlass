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
pub struct FrameGeometry {
    pub width: i32,
    pub height: i32,
    pub plot: RECT,
    pub needs_padding: bool,
}
/// Find a client size whose chart has the requested physical dimensions.
/// At minimum-size constraints, keep the chart exact and leave spare padding.
pub fn same_plot_geometry(
    plot_width: i32,
    plot_height: i32,
    scale: f64,
    focus: bool,
    network: bool,
    preferred: (i32, i32),
    mut stats_fit: impl FnMut(i32) -> bool,
) -> FrameGeometry {
    let px = |v: f64| (v * scale).round() as i32;
    let compact_gap = (11. * scale).round().min(12.) as i32 / 2 + 2;
    let widths = [
        2 * px(2.),
        px(36.) + if network { px(58.) } else { px(9.) },
        px(36.) + if network { px(58.) } else { 4 },
        29 + if network { 39 } else { 4 },
    ];
    let heights = [
        2 * px(2.),
        2 * px(10.),
        compact_gap * 2,
        px(17.) + px(25.),
        px(49.) + px(25.),
        px(49.) + px(65.) + px(25.),
    ];
    let mut best = None;
    for extra_width in widths {
        let width = (plot_width + extra_width).max(280);
        let fit = !focus && stats_fit(width);
        for extra_height in heights {
            let height = (plot_height + extra_height).max(60);
            let layout = Layout::new(width, height, scale, focus, fit, network);
            let spare_width = layout.plot.right - layout.plot.left - plot_width;
            let spare_height = layout.plot.bottom - layout.plot.top - plot_height;
            if spare_width < 0 || spare_height < 0 {
                continue;
            }
            let score = (
                spare_width + spare_height,
                (width - preferred.0).abs() + (height - preferred.1).abs(),
            );
            if best
                .as_ref()
                .is_none_or(|(old_score, _)| score < *old_score)
            {
                best = Some((
                    score,
                    FrameGeometry {
                        width,
                        height,
                        plot: RECT {
                            right: layout.plot.left + plot_width,
                            bottom: layout.plot.top + plot_height,
                            ..layout.plot
                        },
                        needs_padding: spare_width != 0 || spare_height != 0,
                    },
                ));
            }
        }
    }
    best.expect("maximum layout margins must contain the requested plot")
        .1
}
impl Layout {
    pub fn new(w: i32, h: i32, scale: f64, focus: bool, stats_fit: bool, network: bool) -> Self {
        let px = |v: f64| (v * scale).round() as i32;
        let axes = !focus && h >= px(90.) && w >= px(220.);
        let vertical_axes = axes || focus;
        // Keep a usable physical chart width even when a tall, narrow window
        // is moved to a high-DPI monitor. Height alone is insufficient.
        let normal_side_margins = px(36.) + if network { px(58.) } else { 4 };
        let compact_axes =
            (focus && h < px(90.)) || ((focus || axes) && w - normal_side_margins < 212);
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
                left: if compact_axes { 29 } else { px(36.) },
                top: if toolbar { px(49.) } else { px(17.) },
                right: w - if compact_axes {
                    if network {
                        39
                    } else {
                        4
                    }
                } else if network {
                    px(58.)
                } else {
                    px(9.)
                },
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
    fn frame_transitions_preserve_chart_dimensions_and_minimums() {
        for scale in [1., 1.25, 1.5, 1.75, 2., 2.5, 3., 4.] {
            for network in [false, true] {
                for stats in [false, true] {
                    for (width, height) in
                        [(280, 60), (280, 400), (420, 400), (960, 480), (1920, 1080)]
                    {
                        for focus in [false, true] {
                            let old = Layout::new(width, height, scale, focus, stats, network);
                            let pw = old.plot.right - old.plot.left;
                            let ph = old.plot.bottom - old.plot.top;
                            assert!(
                                pw > 0 && ph > 0,
                                "empty chart: {width}x{height}, scale {scale}, focus {focus}"
                            );
                            let target = same_plot_geometry(
                                pw,
                                ph,
                                scale,
                                !focus,
                                network,
                                (width, height),
                                |_| stats,
                            );
                            assert_eq!(target.plot.right - target.plot.left, pw);
                            assert_eq!(target.plot.bottom - target.plot.top, ph);
                            assert!(target.width >= 280 && target.height >= 60);
                            let natural = Layout::new(
                                target.width,
                                target.height,
                                scale,
                                !focus,
                                stats,
                                network,
                            );
                            assert!(
                                target.plot.left >= natural.plot.left
                                    && target.plot.top >= natural.plot.top
                            );
                            assert!(
                                target.plot.right <= natural.plot.right
                                    && target.plot.bottom <= natural.plot.bottom
                            );
                        }
                    }
                }
            }
        }
    }
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
