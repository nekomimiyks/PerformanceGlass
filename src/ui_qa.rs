//! Exercises the real Win32 message handlers and GDI resource lifetime. Runs only
//! with --test-ui; never persists settings or changes system display settings.
use super::*;
unsafe fn double_click_graph(hwnd: HWND, ptr: *mut App) {
    let p = (*ptr).plot;
    let x = (p.left + p.right) / 2;
    let y = (p.top + p.bottom) / 2;
    let was_visible = IsWindowVisible(hwnd);
    SendMessageW(
        hwnd,
        WM_LBUTTONDBLCLK,
        0,
        (x as u16 as u32 | ((y as u16 as u32) << 16)) as isize,
    );
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
    for dpi in [96, 144, 192] {
        (*ptr).scale = dpi as f64 / 96.;
        let fonts = Fonts::new((*ptr).scale);
        SendMessageW((*ptr).tooltip, WM_SETFONT, fonts.small as usize, 0);
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
        if (*ptr).settings.frameless || (client.bottom - client.top) < (*ptr).px(280.) {
            (*ptr).qa_errors.push("normal size was not restored".into());
        }
    }
    (*ptr).scale = saved_scale;
    let fonts = Fonts::new(saved_scale);
    SendMessageW((*ptr).tooltip, WM_SETFONT, fonts.small as usize, 0);
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
            || !text.contains("書込")
            || !text.contains("GPUメモリ 4.20 GiB")
        {
            (*ptr)
                .qa_errors
                .push(format!("history hover missing sensors or disk I/O: {text}"));
        }
    }
    let before = GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS);
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
    let report=format!("UI検証: {}\n96/144/192 DPI、280x60物理px、通常サイズ復帰、表示状態の保持、5期間、2テーマ、温度・読み書きの履歴、GDI 30回描画\nGDI objects: {before} -> {after}\nUser objects (20 theme notifications): {user_before} -> {user_after}\n{}",if (*ptr).qa_errors.is_empty(){"成功"}else{"失敗"},(*ptr).qa_errors.join("\n"));
    if let Some(dir) = &(*ptr).qa_dir {
        let _ = std::fs::write(std::path::Path::new(dir).join("ui-checks.txt"), report);
    }
}
