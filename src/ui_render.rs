use super::*;
use crate::i18n::{self, tr};
use crate::layout::Layout;

#[derive(Default)]
pub(super) struct StrokeBuffer {
    points: Vec<POINT>,
    counts: Vec<u32>,
    peaks: Vec<POINT>,
    peak_counts: Vec<u32>,
}
impl StrokeBuffer {
    fn finish_segment(&mut self, count: &mut u32) {
        if *count == 0 {
            return;
        }
        if *count == 1 {
            let point = *self.points.last().unwrap();
            self.points.push(POINT {
                x: point.x,
                y: point.y + 1,
            });
            *count = 2;
        }
        self.counts.push(*count);
        *count = 0;
    }
}

struct Group {
    series: usize,
    fields: Vec<(&'static str, String)>,
    width: i32,
}
impl App {
    fn measure(&self, dc: HDC, text: &str, font: HFONT) -> i32 {
        unsafe {
            let old = SelectObject(dc, font);
            let s = wide(text);
            let mut extent: SIZE = zeroed();
            GetTextExtentPoint32W(dc, s.as_ptr(), (s.len() - 1) as i32, &mut extent);
            SelectObject(dc, old);
            extent.cx
        }
    }
    fn groups(&self, dc: HDC, s: Sample, compact: bool) -> Vec<Group> {
        let mut groups = Vec::with_capacity(6);
        for i in 0..6 {
            if !self.available[i] {
                continue;
            }
            let mut fields = Vec::with_capacity(3);
            let mut add = |label, value: f32, formatted: String| {
                if value.is_finite() {
                    fields.push((label, formatted));
                }
            };
            match i {
                0 => {
                    add(tr("使用率"), s.values[0], format!("{:.1} %", s.values[0]));
                    if !compact {
                        add(tr("速度"), s.ghz, format!("{:.2} GHz", s.ghz));
                    }
                    add(
                        tr("温度"),
                        s.temperatures[0],
                        format!("{:.0} °C", s.temperatures[0]),
                    );
                }
                1 => {
                    add(tr("使用率"), s.values[1], format!("{:.1} %", s.values[1]));
                    if !compact && s.memory_total > 0. {
                        fields.push((
                            tr("使用量"),
                            format!("{:.1} / {:.1} GiB", s.memory_used, s.memory_total),
                        ));
                    }
                }
                2 => {
                    add(
                        tr("読み込み"),
                        s.disk_io[0],
                        format!("{:.1} MB/s", s.disk_io[0]),
                    );
                    add(
                        tr("書き込み"),
                        s.disk_io[1],
                        format!("{:.1} MB/s", s.disk_io[1]),
                    );
                    if !s.disk_io.iter().any(|v| v.is_finite()) {
                        add(tr("稼働率"), s.values[2], format!("{:.1} %", s.values[2]));
                    }
                }
                3 | 4 => {
                    if s.connected[i - 3] {
                        add(
                            tr("受信 Mbps"),
                            s.traffic[i - 3][0],
                            format!("{:.2}", s.traffic[i - 3][0]),
                        );
                        add(
                            tr("送信 Mbps"),
                            s.traffic[i - 3][1],
                            format!("{:.2}", s.traffic[i - 3][1]),
                        );
                    } else if s.traffic[i - 3][0].is_finite() {
                        fields.push((tr("接続状態"), tr("未接続").into()));
                    }
                }
                _ => {
                    add(tr("使用率"), s.values[5], format!("{:.1} %", s.values[5]));
                    add(
                        tr("GPUメモリ"),
                        s.gpu_memory,
                        format!("{:.2} GiB", s.gpu_memory),
                    );
                    add(
                        tr("温度"),
                        s.temperatures[1],
                        format!("{:.0} °C", s.temperatures[1]),
                    );
                }
            }
            // Do not allocate a tile for missing data, and never print NaN/--.
            if fields.is_empty() {
                continue;
            }
            let content_width = fields
                .iter()
                .map(|(k, v)| {
                    self.measure(dc, k, self.fonts.small).max(self.measure(
                        dc,
                        v,
                        self.fonts.normal,
                    ))
                })
                .sum::<i32>()
                + self.px(10.) * (fields.len() as i32 - 1);
            let width = content_width
                .max(self.measure(dc, names()[i], self.fonts.small) + self.px(15.))
                + self.px(18.);
            groups.push(Group {
                series: i,
                fields,
                width,
            });
        }
        groups
    }
    pub(super) unsafe fn render(&mut self, _hwnd: HWND, dc: HDC, w: i32, h: i32) {
        let profile_start = Instant::now();
        if w <= 0 || h <= 0 {
            return;
        }
        self.fill(
            dc,
            RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            },
            self.palette.bg,
        );
        self.hits.clear();
        let latest = self.data.lock().unwrap().latest();
        let now = metrics::now();
        let available_now = std::array::from_fn::<_, 6, _>(|i| {
            latest.values[i].is_finite()
                || match i {
                    0 => latest.ghz.is_finite() || latest.temperatures[0].is_finite(),
                    1 => latest.memory_total > 0.,
                    2 => latest.disk_io.iter().any(|v| v.is_finite()),
                    3 | 4 => latest.traffic[i - 3][0].is_finite(),
                    5 => latest.temperatures[1].is_finite() || latest.gpu_memory.is_finite(),
                    _ => false,
                }
        });
        for (i, &present) in available_now.iter().enumerate() {
            if present {
                self.availability_until[i] = latest.time + 10.;
            }
            self.available[i] = present || now < self.availability_until[i];
        }
        let mut groups = self.groups(dc, latest, false);
        let width_available = w - self.px(12.);
        if groups.iter().map(|g| g.width).sum::<i32>() > width_available {
            groups = self.groups(dc, latest, true);
        }
        let stats_fit =
            !groups.is_empty() && groups.iter().map(|g| g.width).sum::<i32>() <= width_available;
        let active = std::array::from_fn(|i| self.available[i] && self.settings.visible[i]);
        let has_network = active[3] || active[4];
        let layout = Layout::new(
            w,
            h,
            self.scale,
            self.settings.frameless,
            stats_fit,
            has_network,
        );
        let p = &self.palette;
        if layout.toolbar {
            let y = self.px(3.);
            let bh = self.px(27.);
            let bw = self.px(54.);
            if w >= self.px(470.) {
                for (i, label) in range_labels().iter().enumerate() {
                    let r = RECT {
                        left: self.px(7.) + i as i32 * bw,
                        top: y,
                        right: self.px(7.) + (i as i32 + 1) * bw - self.px(3.),
                        bottom: y + bh,
                    };
                    if self.settings.range == i {
                        self.fill(dc, r, p.selected);
                    }
                    self.text(
                        dc,
                        label,
                        r,
                        self.fonts.small,
                        if self.settings.range == i {
                            p.text
                        } else {
                            p.muted
                        },
                        DT_CENTER,
                    );
                    self.hits.push(Hit {
                        rect: r,
                        command: TIMES[i],
                    });
                }
            } else {
                let r = RECT {
                    left: self.px(7.),
                    top: y,
                    right: self.px(85.),
                    bottom: y + bh,
                };
                self.fill(dc, r, p.selected);
                self.text(
                    dc,
                    &format!("{}  ▾", range_labels()[self.settings.range]),
                    r,
                    self.fonts.small,
                    p.text,
                    DT_CENTER,
                );
                self.hits.push(Hit {
                    rect: r,
                    command: 308,
                });
            }
            let settings_width = (self.measure(dc, tr("設定 ···"), self.fonts.small)
                + self.px(12.))
            .max(self.px(56.));
            let r = RECT {
                left: w - self.px(6.) - settings_width,
                top: y,
                right: w - self.px(6.),
                bottom: y + bh,
            };
            self.text(dc, tr("設定 ···"), r, self.fonts.small, p.muted, DT_CENTER);
            self.hits.push(Hit {
                rect: r,
                command: 306,
            });
            if self.demo && w > self.px(530.) {
                self.label(
                    dc,
                    tr("見本データ"),
                    self.px(300.),
                    y,
                    (r.left - self.px(308.)).max(0),
                    bh,
                    self.fonts.small,
                    p.muted,
                );
            }
        }
        let plot = layout.plot;
        let axis_font = if layout.compact_axes {
            self.fonts.compact
        } else {
            self.fonts.small
        };
        let axis_height = if layout.compact_axes {
            14
        } else {
            self.px(16.)
        };
        let axis_gap = if layout.compact_axes { 3 } else { self.px(7.) };
        self.plot = plot;
        if plot.right <= plot.left || plot.bottom <= plot.top {
            return;
        }
        let range = RANGES[self.settings.range];
        let end = if self.demo { latest.time } else { now };
        self.chart_end = end;
        let graph_start = Instant::now();
        let graph =
            self.data
                .lock()
                .unwrap()
                .graph(end, range, (plot.right - plot.left) as usize, &active);
        self.profile_graph_s += graph_start.elapsed().as_secs_f64();
        // Expand immediately; require 30 seconds before a lower throughput scale.
        if self.network_range != self.settings.range || graph.network_top > self.network_top {
            self.network_top = graph.network_top;
            self.network_range = self.settings.range;
            self.network_lower_since = None;
        } else if graph.network_top < self.network_top {
            let since = self.network_lower_since.get_or_insert_with(Instant::now);
            if since.elapsed().as_secs() >= 30 {
                self.network_top = graph.network_top;
                self.network_lower_since = None;
            }
        } else {
            self.network_lower_since = None;
        }
        let network_top = self.network_top;
        self.fill(dc, plot, p.surface);
        if layout.axes {
            self.label(
                dc,
                tr("使用率 %"),
                plot.left,
                plot.top - self.px(16.),
                self.px(80.),
                self.px(15.),
                self.fonts.small,
                p.muted,
            );
            if has_network {
                self.text(
                    dc,
                    tr("通信 Mbps"),
                    RECT {
                        left: plot.right - self.px(95.),
                        top: plot.top - self.px(16.),
                        right: plot.right,
                        bottom: plot.top - self.px(1.),
                    },
                    self.fonts.small,
                    p.muted,
                    DT_RIGHT,
                );
            }
        }
        for tick in 0..=4 {
            let y = plot.bottom - (plot.bottom - plot.top) * tick / 4;
            self.line(
                dc,
                POINT { x: plot.left, y },
                POINT { x: plot.right, y },
                p.grid,
                1,
                PS_SOLID,
            );
            if layout.vertical_axes && (tick % 2 == 0 || plot.bottom - plot.top > self.px(220.)) {
                self.text(
                    dc,
                    &format!("{}", tick * 25),
                    RECT {
                        left: 0,
                        top: y - axis_height / 2,
                        right: plot.left - axis_gap,
                        bottom: y + axis_height / 2,
                    },
                    axis_font,
                    p.muted,
                    DT_RIGHT,
                );
                if has_network {
                    self.label(
                        dc,
                        &format_axis(network_top * tick as f32 / 4.),
                        plot.right + axis_gap,
                        y - axis_height / 2,
                        w - plot.right - axis_gap,
                        axis_height,
                        axis_font,
                        p.muted,
                    );
                }
            }
        }
        let ticks = if plot.right - plot.left >= self.px(600.) {
            4
        } else {
            2
        };
        for tick in 0..=ticks {
            let x = plot.left + (plot.right - plot.left) * tick / ticks;
            self.line(
                dc,
                POINT { x, y: plot.top },
                POINT { x, y: plot.bottom },
                p.grid,
                1,
                PS_SOLID,
            );
            if layout.axes {
                let label = if tick == ticks {
                    tr("現在").into()
                } else {
                    format_duration(range * (ticks - tick) as u64 / ticks as u64)
                };
                let (left, right, align) = if tick == 0 {
                    (x, x + self.px(95.), DT_LEFT)
                } else if tick == ticks {
                    (x - self.px(95.), x, DT_RIGHT)
                } else {
                    (x - self.px(55.), x + self.px(55.), DT_CENTER)
                };
                self.text(
                    dc,
                    &label,
                    RECT {
                        left,
                        top: plot.bottom + self.px(4.),
                        right,
                        bottom: plot.bottom + self.px(22.),
                    },
                    self.fonts.small,
                    p.muted,
                    align,
                );
            }
        }
        let lines_start = Instant::now();
        let saved = SaveDC(dc);
        IntersectClipRect(dc, plot.left, plot.top, plot.right + 1, plot.bottom + 1);
        let mut strokes = std::mem::take(&mut self.strokes);
        for series in [2, 3, 4, 5, 0, 1] {
            let enabled = active[series];
            if !enabled {
                continue;
            }
            let ceiling = if series == 3 || series == 4 {
                network_top
            } else {
                100.
            };
            let y = |v: f32| {
                plot.bottom
                    - ((v / ceiling).clamp(0., 1.) * (plot.bottom - plot.top) as f32).round() as i32
            };
            let style = if series == 3 {
                PS_DASH
            } else if series == 4 {
                PS_DOT
            } else {
                PS_SOLID
            };
            let pen = CreatePen(
                style,
                if style == PS_SOLID && range < 3600 {
                    self.px(1.4).max(1)
                } else {
                    1
                },
                p.lines[series],
            );
            let old = SelectObject(dc, pen);
            strokes.points.clear();
            strokes.counts.clear();
            strokes.peaks.clear();
            strokes.peak_counts.clear();
            let mut previous_time = None;
            let mut segment = 0;
            for (x, buckets) in graph.buckets.iter().enumerate() {
                let b = buckets[series];
                if b.count == 0 {
                    continue;
                }
                let pt = POINT {
                    x: plot.left + x as i32,
                    y: y(b.sum / b.count as f32),
                };
                if previous_time.is_some_and(|time| b.first_time - time >= 2.5) {
                    strokes.finish_segment(&mut segment);
                }
                strokes.points.push(pt);
                segment += 1;
                if b.count > 1 && y(b.min) != y(b.max) {
                    strokes.peaks.push(POINT {
                        x: pt.x,
                        y: y(b.min),
                    });
                    strokes.peaks.push(POINT {
                        x: pt.x,
                        y: y(b.max),
                    });
                    strokes.peak_counts.push(2);
                }
                previous_time = Some(b.last_time);
            }
            strokes.finish_segment(&mut segment);
            if !strokes.peak_counts.is_empty() {
                // A wide/dashed GDI pen turns a dense envelope into an expensive
                // region union. Use a cosmetic 1px solid pen for the envelope;
                // the average trace above retains its series line style.
                let blend = |shift: u32| -> u32 {
                    let component = (p.lines[series] >> shift) & 255;
                    let bg = (p.surface >> shift) & 255;
                    (component + bg * 3) / 4
                };
                let peak_color = blend(0) | (blend(8) << 8) | (blend(16) << 16);
                let peak_pen = CreatePen(PS_SOLID, 1, peak_color);
                SelectObject(dc, peak_pen);
                PolyPolyline(
                    dc,
                    strokes.peaks.as_ptr(),
                    strokes.peak_counts.as_ptr(),
                    strokes.peak_counts.len() as u32,
                );
                SelectObject(dc, pen);
                DeleteObject(peak_pen);
            }
            // Keep the average line above the subdued min/max envelope.
            if !strokes.counts.is_empty() {
                PolyPolyline(
                    dc,
                    strokes.points.as_ptr(),
                    strokes.counts.as_ptr(),
                    strokes.counts.len() as u32,
                );
            }
            SelectObject(dc, old);
            DeleteObject(pen);
        }
        RestoreDC(dc, saved);
        self.strokes = strokes;
        self.profile_lines_s += lines_start.elapsed().as_secs_f64();
        // Each series owns a disjoint horizontal interval, fixed while the
        // window size is unchanged. Values change Y only; names never collide.
        let title_plot = crate::label_layout::title_area(plot, self.scale);
        let title_enabled: [bool; 6] =
            std::array::from_fn(|i| active[i] && graph.buckets.iter().any(|b| b[i].count > 0));
        let mut title_font = axis_font;
        let mut title_names = &names();
        let mut title_widths = std::array::from_fn(|i| {
            if title_enabled[i] {
                self.measure(dc, title_names[i], title_font) + 2
            } else {
                0
            }
        });
        let tiny_titles = !crate::label_layout::fits(title_plot, &title_widths);
        if tiny_titles {
            title_font = self.fonts.title_tiny;
            title_names = &["CPU", "RAM", "Disk", "LAN", "WiFi", "GPU"];
            title_widths = std::array::from_fn(|i| {
                if title_enabled[i] {
                    self.measure(dc, title_names[i], title_font) + 2
                } else {
                    0
                }
            });
        }
        if self.qa_dir.is_some() && !crate::label_layout::fits(title_plot, &title_widths) {
            self.qa_errors
                .push("title horizontal intervals do not fit".into());
        }
        let title_height = if tiny_titles {
            12
        } else if layout.compact_axes {
            14
        } else {
            self.px(16.)
        };
        let title_lefts = crate::label_layout::horizontal_positions(title_plot, &title_widths);
        let mut targets = Vec::with_capacity(6);
        for series in 0..6 {
            if !title_enabled[series] {
                continue;
            }
            let title_x = ((title_lefts[series] + title_widths[series] / 2 - plot.left).max(0)
                as usize)
                .min(graph.buckets.len().saturating_sub(1));
            if let Some((_, bucket)) = graph
                .buckets
                .iter()
                .enumerate()
                .filter_map(|(x, b)| (b[series].count > 0).then_some((x, b[series])))
                .min_by_key(|(x, _)| x.abs_diff(title_x))
            {
                let ceiling = if series == 3 || series == 4 {
                    network_top
                } else {
                    100.
                };
                let y = plot.bottom
                    - ((bucket.sum / bucket.count as f32 / ceiling).clamp(0., 1.)
                        * (plot.bottom - plot.top) as f32)
                        .round() as i32;
                targets.push((series, y - title_height - 2));
            }
        }
        let titles = crate::label_layout::place(title_plot, &targets, &title_widths, title_height);
        for (series, rect) in titles {
            self.fill(dc, rect, p.surface);
            self.text(
                dc,
                title_names[series],
                rect,
                title_font,
                p.lines[series],
                DT_CENTER,
            );
        }
        if layout.stats {
            let top = layout.stats_y;
            self.line(
                dc,
                POINT { x: 0, y: top },
                POINT { x: w, y: top },
                p.grid,
                1,
                PS_SOLID,
            );
            let used = groups.iter().map(|g| g.width).sum::<i32>();
            let extra = (width_available - used) / groups.len() as i32;
            let mut x = self.px(6.);
            for (index, group) in groups.iter().enumerate() {
                let width = group.width + extra;
                let s = group.series;
                let color = if self.settings.visible[s] {
                    p.lines[s]
                } else {
                    p.muted
                };
                self.line(
                    dc,
                    POINT {
                        x: x + self.px(6.),
                        y: top + self.px(13.),
                    },
                    POINT {
                        x: x + self.px(16.),
                        y: top + self.px(13.),
                    },
                    color,
                    self.px(2.),
                    if s == 3 {
                        PS_DASH
                    } else if s == 4 {
                        PS_DOT
                    } else {
                        PS_SOLID
                    },
                );
                self.label(
                    dc,
                    names()[s],
                    x + self.px(21.),
                    top + self.px(4.),
                    width - self.px(24.),
                    self.px(17.),
                    self.fonts.small,
                    if self.settings.visible[s] {
                        p.text
                    } else {
                        p.muted
                    },
                );
                let mut fx = x + self.px(8.);
                for (key, value) in &group.fields {
                    let fw = self.measure(dc, key, self.fonts.small).max(self.measure(
                        dc,
                        value,
                        self.fonts.normal,
                    ));
                    self.label(
                        dc,
                        key,
                        fx,
                        top + self.px(24.),
                        fw + self.px(2.),
                        self.px(15.),
                        self.fonts.small,
                        p.muted,
                    );
                    self.label(
                        dc,
                        value,
                        fx,
                        top + self.px(41.),
                        fw + self.px(2.),
                        self.px(18.),
                        self.fonts.normal,
                        p.text,
                    );
                    fx += fw + self.px(10.);
                }
                self.hits.push(Hit {
                    rect: RECT {
                        left: x,
                        top,
                        right: x + width,
                        bottom: h,
                    },
                    command: 201 + s,
                });
                x += width;
                if index + 1 < groups.len() {
                    self.line(
                        dc,
                        POINT {
                            x,
                            y: top + self.px(6.),
                        },
                        POINT {
                            x,
                            y: h - self.px(6.),
                        },
                        p.grid,
                        1,
                        PS_SOLID,
                    );
                }
            }
        }
        if self.data.lock().unwrap().samples.is_empty()
            && plot.right - plot.left > self.px(200.)
            && plot.bottom - plot.top > self.px(40.)
        {
            self.text(
                dc,
                tr("計測を準備しています…"),
                plot,
                self.fonts.small,
                p.muted,
                DT_CENTER,
            );
        }
        self.profile_frame_s += profile_start.elapsed().as_secs_f64();
    }
    pub(super) fn hover_text(&self, x: i32) -> Option<String> {
        if x < self.plot.left || x > self.plot.right || self.plot.right <= self.plot.left {
            return None;
        }
        let end = self.chart_end;
        let range = RANGES[self.settings.range];
        let target = end - range as f64
            + (x - self.plot.left) as f64 / (self.plot.right - self.plot.left) as f64
                * range as f64;
        let s = self.data.lock().unwrap().nearest(target)?;
        if (s.time - target).abs() > 2.5 {
            return None;
        }
        let mut rows = vec![format!(
            "{}{}",
            i18n::message("{}前", &format_age((end - s.time).max(0.) as u64)),
            if self.demo { tr("（見本）") } else { "" }
        )];
        for i in 0..6 {
            if !self.available[i] {
                continue;
            }
            let mut fields = Vec::new();
            if s.values[i].is_finite() && i != 3 && i != 4 {
                fields.push(format!(
                    "{} {:.1}%",
                    if i == 2 {
                        tr("稼働率")
                    } else {
                        tr("使用率")
                    },
                    s.values[i]
                ));
            }
            match i {
                0 => {
                    if s.ghz.is_finite() {
                        fields.push(format!("{} {:.2}GHz", tr("速度"), s.ghz));
                    }
                    if s.temperatures[0].is_finite() {
                        fields.push(format!("{} {:.0}°C", tr("温度"), s.temperatures[0]));
                    }
                }
                1 => {
                    if s.memory_total > 0. {
                        fields.push(format!("{:.1} / {:.1} GiB", s.memory_used, s.memory_total));
                    }
                }
                2 => {
                    if s.disk_io[0].is_finite() {
                        fields.push(format!("{} {:.1}MB/s", tr("読み込み"), s.disk_io[0]));
                    }
                    if s.disk_io[1].is_finite() {
                        fields.push(format!("{} {:.1}MB/s", tr("書き込み"), s.disk_io[1]));
                    }
                }
                3 | 4 => {
                    if s.traffic[i - 3][0].is_finite() {
                        fields.push(if s.connected[i - 3] {
                            format!(
                                "{} {:.2} / {} {:.2} Mbps",
                                tr("受信"),
                                s.traffic[i - 3][0],
                                tr("送信"),
                                s.traffic[i - 3][1]
                            )
                        } else {
                            tr("未接続").into()
                        });
                    }
                }
                5 => {
                    if s.gpu_memory.is_finite() {
                        fields.push(format!(
                            "{} {:.2} GiB ({})",
                            tr("GPUメモリ"),
                            s.gpu_memory,
                            tr("専用")
                        ));
                    }
                    if s.temperatures[1].is_finite() {
                        fields.push(format!("{} {:.0}°C", tr("温度"), s.temperatures[1]));
                    }
                }
                _ => {}
            }
            if !fields.is_empty() {
                rows.push(format!("{}   {}", names()[i], fields.join("  ")));
            }
        }
        if self.available[3] || self.available[4] {
            rows.push(format!(
                "{} {:.2} Mbps ({})",
                tr("通信の目盛り上限"),
                self.network_top,
                tr("送受信合計")
            ));
        }
        if s.disk_io[0].is_finite() || s.disk_io[1].is_finite() {
            rows.push(tr("ディスク: 稼働率は最大、読み書きは全物理ディスク合計").into());
        }
        Some(rows.join("\n"))
    }
}
fn format_axis(v: f32) -> String {
    if v >= 1000. {
        format!("{:.1}k", v / 1000.)
    } else if v >= 10. {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
    }
}
