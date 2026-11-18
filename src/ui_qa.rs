//! Exercises the real Win32 message handlers and GDI resource lifetime. Runs only
//! with --test-ui; never persists settings or changes system display settings.
use super::*;

#[derive(Default)]
struct TooltipVisibility {
    show: u32,
    hide: u32,
}

unsafe extern "system" fn record_tooltip(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    if msg == WM_SHOWWINDOW {
        let events = data as *mut TooltipVisibility;
        if w != 0 {
            (*events).show += 1;
        } else {
            (*events).hide += 1;
        }
    }
    DefSubclassProc(hwnd, msg, w, l)
}

unsafe fn double_click_graph(hwnd: HWND, ptr: *mut App) {
    (*ptr).render_now(hwnd);
    let p = (*ptr).plot;
    let mut before = POINT {
        x: p.left,
        y: p.top,
    };
    ClientToScreen(hwnd, &mut before);
    let x = (p.left + p.right) / 2;
    let y = (p.top + p.bottom) / 2;
    let was_visible = IsWindowVisible(hwnd);
    SendMessageW(
        hwnd,
        WM_LBUTTONDBLCLK,
        0,
        (x as u16 as u32 | ((y as u16 as u32) << 16)) as isize,
    );
    (*ptr).render_now(hwnd);
    let current = (*ptr).plot;
    let mut after = POINT {
        x: current.left,
        y: current.top,
    };
    ClientToScreen(hwnd, &mut after);
    if (p.right - p.left, p.bottom - p.top, before.x, before.y)
        != (
            current.right - current.left,
            current.bottom - current.top,
            after.x,
            after.y,
        )
    {
        (*ptr).qa_errors.push(format!(
            "frame toggle changed plot geometry: {}x{} @{},{} -> {}x{} @{},{}",
            p.right - p.left,
            p.bottom - p.top,
            before.x,
            before.y,
            current.right - current.left,
            current.bottom - current.top,
            after.x,
            after.y
        ));
    }
    if IsWindowVisible(hwnd) != was_visible {
        (*ptr)
            .qa_errors
            .push("frame toggle changed window visibility".into());
    }
}
pub(super) unsafe fn run(hwnd: HWND, ptr: *mut App) {
    if crate::packaged_startup::is_packaged() && !crate::packaged_startup::query_succeeded() {
        (*ptr)
            .qa_errors
            .push("packaged startup task query failed or timed out".into());
    }
    let saved_scale = (*ptr).scale;
    let saved_theme = (*ptr).theme_override;
    let mut original: RECT = zeroed();
    GetWindowRect(hwnd, &mut original);
    let snapshot = |app: &mut App, name: &str| {
        if let Some(dir) = &app.qa_dir {
            let dir = std::path::Path::new(dir);
            let _ = std::fs::create_dir_all(dir);
            let path = dir.join(name);
            if let Err(e) = save_bitmap(
                app.buffer.dc,
                app.buffer.bitmap,
                app.buffer.width,
                app.buffer.height,
                &path.to_string_lossy(),
            ) {
                app.qa_errors.push(format!("{name}: {e}"));
            }
        }
    };
    if (*ptr).settings.frameless {
        double_click_graph(hwnd, ptr);
    }
    for dpi in [96, 120, 144, 192, 288, 384] {
        (*ptr).scale = dpi as f64 / 96.;
        let fonts = Fonts::new((*ptr).scale);

        (*ptr).fonts = fonts;
        for (name, width, height) in [
            ("normal", 960, 480),
            ("narrow680", 680, 400),
            ("narrow420", 420, 400),
            ("narrow280", 280, 400),
        ] {
            SetWindowPos(
                hwnd,
                null_mut(),
                original.left,
                original.top,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            (*ptr).render_now(hwnd);
            let p = (*ptr).plot;
            if p.right <= p.left || p.bottom <= p.top {
                (*ptr)
                    .qa_errors
                    .push(format!("empty plot: {name}, DPI {dpi}"));
            }
            for (i, a) in (*ptr).hits.iter().enumerate() {
                for b in (*ptr).hits.iter().skip(i + 1) {
                    if a.rect.left < b.rect.right
                        && a.rect.right > b.rect.left
                        && a.rect.top < b.rect.bottom
                        && a.rect.bottom > b.rect.top
                    {
                        (*ptr)
                            .qa_errors
                            .push(format!("overlapping controls: {name}, DPI {dpi}"));
                    }
                }
            }
            snapshot(&mut *ptr, &format!("{name}-dpi{dpi}.bmp"));
        }
        double_click_graph(hwnd, ptr);
        if !(*ptr).settings.frameless
            || (GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_CAPTION) != 0
        {
            (*ptr)
                .qa_errors
                .push("double click did not hide chrome".into());
        }
        SetWindowPos(
            hwnd,
            null_mut(),
            original.left,
            original.top,
            280,
            270,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        (*ptr).render_now(hwnd);
        if (*ptr).plot.right <= (*ptr).plot.left || (*ptr).plot.bottom <= (*ptr).plot.top {
            (*ptr)
                .qa_errors
                .push(format!("tall narrow graph disappeared at DPI {dpi}"));
        }
        SetWindowPos(
            hwnd,
            null_mut(),
            original.left,
            original.top,
            280,
            60,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        (*ptr).render_now(hwnd);
        let mut client: RECT = zeroed();
        GetClientRect(hwnd, &mut client);
        if client.bottom != 60 || client.right != 280 {
            (*ptr).qa_errors.push(format!(
                "60 physical pixel window failed at DPI {dpi}: {}x{}",
                client.right, client.bottom
            ));
        }
        if !(*ptr).hits.is_empty() {
            (*ptr)
                .qa_errors
                .push("controls remain in graph-only mode".into());
        }
        let minimum = minimum_window_size((*ptr).scale, true);
        if minimum.x != 280 || minimum.y != 60 {
            (*ptr)
                .qa_errors
                .push("graph-only minimum must be physical 280x60".into());
        }
        // Each edge/corner must remain a resize target after chrome removal.
        let mut r: RECT = zeroed();
        GetWindowRect(hwnd, &mut r);
        for (x, y, expected) in [
            (r.left + 1, r.top + 30, HTLEFT),
            (r.right - 1, r.top + 30, HTRIGHT),
            (r.left + 140, r.top + 1, HTTOP),
            (r.left + 140, r.bottom - 1, HTBOTTOM),
            (r.right - 1, r.bottom - 1, HTBOTTOMRIGHT),
        ] {
            let hit = SendMessageW(
                hwnd,
                WM_NCHITTEST,
                0,
                (x as u16 as u32 | ((y as u16 as u32) << 16)) as isize,
            );
            if hit != expected as isize {
                (*ptr)
                    .qa_errors
                    .push(format!("resize hit test failed: {expected} -> {hit}"));
            }
        }
        let mut limits: MINMAXINFO = zeroed();
        SendMessageW(hwnd, WM_GETMINMAXINFO, 0, &mut limits as *mut _ as isize);
        if limits.ptMinTrackSize.x != 280 || limits.ptMinTrackSize.y != 60 {
            (*ptr)
                .qa_errors
                .push("minimum tracking dimensions not enforced".into());
        }
        snapshot(&mut *ptr, &format!("thin60-dpi{dpi}.bmp"));
        double_click_graph(hwnd, ptr);
        GetWindowRect(hwnd, &mut client);
        if (*ptr).settings.frameless {
            (*ptr)
                .qa_errors
                .push("normal frame was not restored".into());
        }
    }
    (*ptr).scale = saved_scale;
    let fonts = Fonts::new(saved_scale);

    (*ptr).fonts = fonts;
    SetWindowPos(
        hwnd,
        null_mut(),
        original.left,
        original.top,
        original.right - original.left,
        original.bottom - original.top,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    for id in 101..=105 {
        SendMessageW(hwnd, WM_COMMAND, id, 0);
        (*ptr).render_now(hwnd);
    }
    SendMessageW(hwnd, WM_COMMAND, 102, 0);
    for theme in [false, true] {
        (*ptr).theme_override = Some(theme);
        (*ptr).theme(hwnd);
        (*ptr).render_now(hwnd);
        snapshot(
            &mut *ptr,
            if theme {
                "normal-dark.bmp"
            } else {
                "normal-light.bmp"
            },
        );
    }
    (*ptr).theme_override = saved_theme;
    (*ptr).theme(hwnd);
    (*ptr).render_now(hwnd);
    if (*ptr).demo {
        let text = (*ptr).hover_text((*ptr).plot.right).unwrap_or_default();
        if !text.contains("62°C")
            || !text.contains("25.8MB/s")
            || !text.contains(crate::i18n::tr("書き込み"))
            || !text.contains(&format!("{} 4.20 GiB", crate::i18n::tr("GPUメモリ")))
        {
            (*ptr)
                .qa_errors
                .push(format!("history hover missing sensors or disk I/O: {text}"));
        }
    }
    if (*ptr).demo {
        // Simulate an adapter whose last recorded point is 30 seconds old.
        let mut history = History::new();
        let mut recorded = Sample {
            time: 30.,
            ..Sample::default()
        };
        recorded.values[4] = 10.;
        recorded.traffic[1] = [8., 2.];
        recorded.connected[1] = true;
        history.push(recorded);
        history.push(Sample {
            time: 60.,
            ..Sample::default()
        });
        let old_data = std::mem::replace(&mut (*ptr).data, Arc::new(Mutex::new(history)));
        let old_range = (*ptr).settings.range;
        let old_visible = (*ptr).settings.visible;
        (*ptr).settings.range = 0;
        (*ptr).settings.visible = [true; 6];
        (*ptr).availability_until = [0.; 6];
        (*ptr).render_now(hwnd);
        let plot = (*ptr).plot;
        let old_point = (plot.left + plot.right - 1) / 2;
        if (*ptr).available[4]
            || !(*ptr).chart_available[4]
            || !(*ptr)
                .graph
                .buckets
                .iter()
                .any(|column| column[4].count > 0)
            || !(*ptr)
                .hover_text(old_point)
                .is_some_and(|hint| hint.contains("Wi-Fi") && hint.contains("8.00"))
        {
            (*ptr)
                .qa_errors
                .push("removed adapter lost its recorded graph or tooltip".into());
        }
        (*ptr).data = old_data;
        (*ptr).settings.range = old_range;
        (*ptr).settings.visible = old_visible;
        (*ptr).render_now(hwnd);
    }
    if (*ptr).demo {
        let mut visibility = TooltipVisibility::default();
        let tooltip = (*ptr).tooltip.hwnd;
        let installed = SetWindowSubclass(
            tooltip,
            Some(record_tooltip),
            1,
            &mut visibility as *mut _ as usize,
        );
        if installed != 0 {
            let plot = (*ptr).plot;
            let middle = (plot.left + plot.right) / 2;
            let mut cursor = POINT {
                x: middle,
                y: (plot.top + plot.bottom) / 2,
            };
            ClientToScreen(hwnd, &mut cursor);
            let style = tooltip::TooltipStyle {
                font: (*ptr).fonts.small,
                background: (*ptr).palette.surface,
                foreground: (*ptr).palette.text,
                border: (*ptr).palette.grid,
                scale: (*ptr).scale,
            };
            let mut first: RECT = zeroed();
            let mut moved: RECT = zeroed();
            for offset in [0, 20, 40, -25, 10] {
                let text = (*ptr).hover_text(middle + offset).unwrap();
                (*ptr).tooltip.update(
                    text,
                    POINT {
                        x: cursor.x + offset,
                        y: cursor.y,
                    },
                    style,
                );
                if offset == 0 {
                    GetWindowRect(tooltip, &mut first);
                }
                if offset == 40 {
                    GetWindowRect(tooltip, &mut moved);
                }
            }
            if first.left == moved.left
                || IsWindowVisible(tooltip) == 0
                || (visibility.show, visibility.hide) != (1, 0)
            {
                (*ptr).qa_errors.push(format!(
                    "tooltip movement should stay visible: show {}, hide {}",
                    visibility.show, visibility.hide
                ));
            }
            let hit = WindowFromPoint(POINT {
                x: moved.left + 2,
                y: moved.top + 2,
            });
            if hit == tooltip {
                (*ptr)
                    .qa_errors
                    .push("tooltip intercepted the pointer".into());
            }
            SendMessageW(hwnd, WM_MOUSELEAVE, 0, 0);
            if IsWindowVisible(tooltip) != 0 || (*ptr).hover.is_some() {
                (*ptr)
                    .qa_errors
                    .push("tooltip did not hide after mouse leave".into());
            }
            RemoveWindowSubclass(tooltip, Some(record_tooltip), 1);
        } else {
            (*ptr)
                .qa_errors
                .push("could not observe tooltip visibility".into());
        }
    }
    let before = GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS);
    let saved_language = crate::i18n::language();
    let saved_preference = (*ptr).settings.language;
    for _ in 0..2 {
        for (i, language) in crate::i18n::LANGUAGES.iter().enumerate() {
            SendMessageW(hwnd, WM_COMMAND, 401 + i, 0);
            if crate::i18n::language() != *language || (*ptr).settings.language != Some(*language) {
                (*ptr)
                    .qa_errors
                    .push("language command did not apply".into());
            }
            (*ptr).render_now(hwnd);
        }
    }
    SendMessageW(hwnd, WM_COMMAND, 400, 0);
    if (*ptr).settings.language.is_some() {
        (*ptr)
            .qa_errors
            .push("automatic language command did not apply".into());
    }
    SendMessageW(hwnd, WM_COMMAND, 401 + saved_language as usize, 0);
    (*ptr).settings.language = saved_preference;
    let user_before = GetGuiResources(GetCurrentProcess(), GR_USEROBJECTS);
    for _ in 0..20 {
        SendMessageW(hwnd, WM_THEMECHANGED, 0, 0);
    }
    let user_after = GetGuiResources(GetCurrentProcess(), GR_USEROBJECTS);
    if user_after > user_before {
        (*ptr).qa_errors.push(format!(
            "theme changes leaked tooltip windows: {user_before} -> {user_after}"
        ));
    }
    for _ in 0..30 {
        (*ptr).render_now(hwnd);
    }
    let after = GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS);
    if after > before + 1 {
        (*ptr)
            .qa_errors
            .push(format!("GDI objects leaked: {before} -> {after}"));
    }
    if (*ptr).demo {
        if (*ptr).settings.tray_enabled || !(*ptr).tray.hWnd.is_null() {
            (*ptr)
                .qa_errors
                .push("system tray should be off by default".into());
        }
        SendMessageW(hwnd, WM_COMMAND, 310, 0);
        if !(*ptr).settings.tray_enabled || (*ptr).tray.hWnd != hwnd {
            (*ptr)
                .qa_errors
                .push("system tray could not be enabled".into());
        }
        let metric = (*ptr).settings.taskbar_metric;
        SendMessageW(hwnd, WM_COMMAND, taskbar_graph::MENU_BASE + 1, 0);
        let cpu_icon = (*ptr).graph_icon;
        update_taskbar_icon(hwnd, ptr);
        if cpu_icon.is_null() || (*ptr).graph_icon != cpu_icon || (*ptr).tray.hIcon != cpu_icon {
            (*ptr)
                .qa_errors
                .push("CPU mini graph failed to create or reuse its icon".into());
        }
        let gpu_memory = METRICS
            .iter()
            .position(|metric| metric.key == "gpu_memory")
            .unwrap();
        SendMessageW(
            hwnd,
            WM_COMMAND,
            taskbar_graph::MENU_BASE + 1 + gpu_memory,
            0,
        );
        if (*ptr).graph_icon.is_null() || (*ptr).graph_icon == cpu_icon {
            (*ptr)
                .qa_errors
                .push("mini graph did not update for GPU memory".into());
        }
        SendMessageW(hwnd, WM_COMMAND, taskbar_graph::MENU_BASE, 0);
        if !(*ptr).graph_icon.is_null() || (*ptr).tray.hIcon != (*ptr).icon {
            (*ptr)
                .qa_errors
                .push("mini graph did not restore the standard icon".into());
        }
        (*ptr).settings.taskbar_metric = metric;
        update_taskbar_icon(hwnd, ptr);
        SendMessageW(hwnd, WM_COMMAND, 310, 0);
        if (*ptr).settings.tray_enabled {
            (*ptr)
                .qa_errors
                .push("system tray could not be disabled".into());
        }
    }
    let report=format!("UI検証: {}\n96/120/144/192/288/384 DPI、280x60物理px、枠切替時のグラフ寸法と画面座標保持、表示状態の保持、5期間、2テーマ、温度・読み書きの履歴、GDI 30回描画\nGDI objects: {before} -> {after}\nUser objects (20 theme notifications): {user_before} -> {user_after}\n{}",if (*ptr).qa_errors.is_empty(){"成功"}else{"失敗"},(*ptr).qa_errors.join("\n"));
    if let Some(dir) = &(*ptr).qa_dir {
        let _ = std::fs::write(std::path::Path::new(dir).join("ui-checks.txt"), report);
    }
}
