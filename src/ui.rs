use crate::i18n::{self, tr};
use crate::{
    history::*,
    metrics,
    settings::{self, Settings},
};
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::{Arc, Mutex},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::{LibraryLoader::*, Registry::*, Threading::*},
    UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
};

#[path = "ui_render.rs"]
mod rendering;
#[path = "ui_qa.rs"]
mod verification;

const CLASS: &str = "PerformanceGlass.Native.Window";
const WM_MOUSELEAVE: u32 = 0x02A3;
const TRAY_MESSAGE: u32 = WM_APP + 1;
const TEST_UI: u32 = WM_APP + 2;
const TIMES: [usize; 5] = [101, 102, 103, 104, 105];
/// Minimum drawable area is physical 280x60 even at high DPI.
unsafe fn minimum_window_size(scale: f64, frameless: bool) -> POINT {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 280,
        bottom: 60,
    };
    if !frameless {
        AdjustWindowRectExForDpi(
            &mut r,
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            (96. * scale).round() as u32,
        );
    }
    POINT {
        x: r.right - r.left,
        y: r.bottom - r.top,
    }
}
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}
fn hex(v: u32) -> u32 {
    rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

struct Palette {
    bg: u32,
    surface: u32,
    text: u32,
    muted: u32,
    grid: u32,
    selected: u32,
    lines: [u32; 6],
}
impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                bg: hex(0x11151c),
                surface: hex(0x1a202b),
                text: hex(0xe5edf8),
                muted: hex(0x96a4b8),
                grid: hex(0x2c3544),
                selected: hex(0x243d5b),
                lines: [0x60a5fa, 0xc084fc, 0xfbbf24, 0x34d399, 0x22d3ee, 0xfb7185].map(hex),
            }
        } else {
            Self {
                bg: hex(0xf7f9fc),
                surface: hex(0xffffff),
                text: hex(0x182233),
                muted: hex(0x526178),
                grid: hex(0xdce3ed),
                selected: hex(0xe0ecff),
                lines: [0x2563eb, 0x9333ea, 0xa16207, 0x047857, 0x087e98, 0xc52b50].map(hex),
            }
        }
    }
}
struct Fonts {
    small: HFONT,
    normal: HFONT,
    compact: HFONT,
    title_tiny: HFONT,
}
impl Fonts {
    unsafe fn new(scale: f64) -> Self {
        let font = |size: f64, weight: i32, face: &str| {
            CreateFontW(
                -(size * scale).round() as i32,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                0,
                0,
                CLEARTYPE_QUALITY as u32,
                0,
                wide(face).as_ptr(),
            )
        };
        Self {
            small: font(11., 400, i18n::font_name()),
            normal: font(12., 400, i18n::font_name()),
            compact: font((11. * scale).min(12.) / scale, 400, i18n::font_name()),
            // Compact trace names are shared Latin abbreviations, with identical
            // metrics in every language so all six fit in 280 physical pixels.
            title_tiny: font(10. / scale, 400, "Segoe UI"),
        }
    }
}
impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            for f in [self.small, self.normal, self.compact, self.title_tiny] {
                DeleteObject(f);
            }
        }
    }
}
struct BackBuffer {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    width: i32,
    height: i32,
}
impl BackBuffer {
    fn new() -> Self {
        Self {
            dc: null_mut(),
            bitmap: null_mut(),
            old: null_mut(),
            width: 0,
            height: 0,
        }
    }
    unsafe fn ensure(&mut self, source: HDC, w: i32, h: i32) {
        if self.width == w && self.height == h && !self.dc.is_null() {
            return;
        }
        self.clear();
        self.dc = CreateCompatibleDC(source);
        self.bitmap = CreateCompatibleBitmap(source, w.max(1), h.max(1));
        self.old = SelectObject(self.dc, self.bitmap);
        self.width = w;
        self.height = h;
    }
    unsafe fn clear(&mut self) {
        if !self.dc.is_null() {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
        self.dc = null_mut();
        self.width = 0;
        self.height = 0;
    }
}
impl Drop for BackBuffer {
    fn drop(&mut self) {
        unsafe {
            self.clear();
        }
    }
}
#[derive(Clone, Copy)]
struct Hit {
    rect: RECT,
    command: usize,
}
#[derive(Clone, Copy)]
struct DragStart {
    cursor: POINT,
    rect: RECT,
    moved: bool,
}
struct App {
    data: Arc<Mutex<History>>,
    settings: Settings,
    palette: Palette,
    scale: f64,
    fonts: Fonts,
    buffer: BackBuffer,
    strokes: rendering::StrokeBuffer,
    hits: Vec<Hit>,
    available: [bool; 6],
    availability_until: [f64; 6],
    chart_end: f64,
    network_top: f32,
    network_range: usize,
    network_lower_since: Option<Instant>,
    plot: RECT,
    hover: Option<i32>,
    tray: NOTIFYICONDATAW,
    icon: HICON,
    taskbar_created: u32,
    snapshot: Option<String>,
    test_ui: bool,
    started: Instant,
    cpu_start: f64,
    diagnostic: Option<String>,
    auto_exit: Option<u64>,
    demo: bool,
    tooltip: HWND,
    tooltip_text: Vec<u16>,
    tooltip_on: bool,
    theme_updating: bool,
    applied_dark: Option<bool>,
    drag_start: Option<DragStart>,
    persist: bool,
    theme_override: Option<bool>,
    forced_dpi: bool,
    benchmark_render: bool,
    paint_count: u64,
    profile_graph_s: f64,
    profile_lines_s: f64,
    profile_frame_s: f64,
    qa_dir: Option<String>,
    qa_errors: Vec<String>,
}
impl App {
    unsafe fn render_now(&mut self, hwnd: HWND) {
        let dc = GetDC(hwnd);
        let mut rect: RECT = zeroed();
        GetClientRect(hwnd, &mut rect);
        self.buffer.ensure(dc, rect.right, rect.bottom);
        self.render(hwnd, self.buffer.dc, rect.right, rect.bottom);
        self.paint_count += 1;
        ReleaseDC(hwnd, dc);
    }
    fn save_settings(&self) {
        if self.persist {
            self.settings.save();
        }
    }
    unsafe fn update_position(&mut self, hwnd: HWND) {
        if IsIconic(hwnd) != 0 || IsZoomed(hwnd) != 0 {
            return;
        }
        let mut rect: RECT = zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return;
        }
        self.settings.x = rect.left;
        self.settings.y = rect.top;
        self.settings.width = ((rect.right - rect.left) as f64 / self.scale).round() as i32;
        self.settings.height = ((rect.bottom - rect.top) as f64 / self.scale).round() as i32;
        if !self.settings.frameless && self.settings.height >= 280 {
            self.settings.normal_width = self.settings.width;
            self.settings.normal_height = self.settings.height;
        }
    }
    unsafe fn tool_info(&mut self, hwnd: HWND) -> TTTOOLINFOW {
        TTTOOLINFOW {
            cbSize: size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_TRACK | TTF_ABSOLUTE | TTF_TRANSPARENT,
            hwnd,
            uId: 1,
            lpszText: self.tooltip_text.as_mut_ptr(),
            ..zeroed()
        }
    }
    unsafe fn create_tooltip(&mut self, hwnd: HWND) {
        if !self.tooltip.is_null() {
            return;
        }
        self.tooltip = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            wide("tooltips_class32").as_ptr(),
            null(),
            WS_POPUP | TTS_NOPREFIX | TTS_ALWAYSTIP | TTS_NOANIMATE | TTS_NOFADE,
            0,
            0,
            0,
            0,
            hwnd,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if self.tooltip.is_null() {
            return;
        }
        let info = self.tool_info(hwnd);
        SendMessageW(self.tooltip, TTM_ADDTOOLW, 0, &info as *const _ as isize);
        SendMessageW(self.tooltip, TTM_SETMAXTIPWIDTH, 0, self.px(560.) as isize);
        SendMessageW(self.tooltip, WM_SETFONT, self.fonts.small as usize, 0);
        SetWindowTheme(self.tooltip, wide("").as_ptr(), wide("").as_ptr());
    }
    unsafe fn tooltip_hide(&mut self, hwnd: HWND) {
        if self.tooltip_on && !self.tooltip.is_null() {
            // A synchronous control message can re-enter the owner window.
            self.tooltip_on = false;
            let info = self.tool_info(hwnd);
            SendMessageW(
                self.tooltip,
                TTM_TRACKACTIVATE,
                0,
                &info as *const _ as isize,
            );
        }
    }
    unsafe fn tooltip_update(&mut self, hwnd: HWND, x: i32) {
        if self.tooltip.is_null() {
            return;
        }
        let Some(text) = self.hover_text(x) else {
            self.tooltip_hide(hwnd);
            return;
        };
        self.tooltip_text = wide(&text);
        let info = self.tool_info(hwnd);
        SendMessageW(
            self.tooltip,
            TTM_UPDATETIPTEXTW,
            0,
            &info as *const _ as isize,
        );
        let mut position: POINT = zeroed();
        GetCursorPos(&mut position);
        SendMessageW(
            self.tooltip,
            TTM_TRACKPOSITION,
            0,
            ((position.x + self.px(12.)) as u16 as u32
                | ((position.y + self.px(16.)) as u16 as u32) << 16) as isize,
        );
        SendMessageW(
            self.tooltip,
            TTM_TRACKACTIVATE,
            1,
            &info as *const _ as isize,
        );
        self.tooltip_on = true;
    }
    fn px(&self, v: f64) -> i32 {
        (v * self.scale).round() as i32
    }
    unsafe fn theme(&mut self, hwnd: HWND) {
        if self.theme_updating {
            return;
        }
        self.theme_updating = true;
        let dark = self.theme_override.unwrap_or_else(|| {
            metrics::registry_dword(
                HKEY_CURRENT_USER,
                r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                "AppsUseLightTheme",
            )
            .unwrap_or(1)
                == 0
        });
        self.palette = Palette::new(dark);
        let enabled = dark as i32;
        if self.applied_dark != Some(dark) {
            self.applied_dark = Some(dark);
            DwmSetWindowAttribute(hwnd, 20, &enabled as *const _ as _, 4);
        }
        if !self.tooltip.is_null() {
            SendMessageW(
                self.tooltip,
                TTM_SETTIPBKCOLOR,
                self.palette.surface as usize,
                0,
            );
            SendMessageW(
                self.tooltip,
                TTM_SETTIPTEXTCOLOR,
                self.palette.text as usize,
                0,
            );
        }
        InvalidateRect(hwnd, null(), 0);
        self.theme_updating = false;
    }
    unsafe fn tray_add(&mut self, hwnd: HWND) -> bool {
        self.tray.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        self.tray.hWnd = hwnd;
        self.tray.uID = 1;
        self.tray.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        self.tray.uCallbackMessage = TRAY_MESSAGE;
        self.tray.hIcon = self.icon;
        let tip = wide(&format!("Performance Glass — {}", tr("クリックして表示")));
        self.tray.szTip[..tip.len()].copy_from_slice(&tip);
        Shell_NotifyIconW(NIM_ADD, &self.tray) != 0
    }
    unsafe fn hide(&mut self, hwnd: HWND) {
        self.hover = None;
        self.tooltip_hide(hwnd);
        KillTimer(hwnd, 1);
        // Never leave an unreachable hidden process when the notification area rejects an icon.
        if Shell_NotifyIconW(NIM_MODIFY, &self.tray) != 0 || self.tray_add(hwnd) {
            ShowWindow(hwnd, SW_HIDE);
        } else {
            ShowWindow(hwnd, SW_MINIMIZE);
        }
    }
    unsafe fn show(&mut self, hwnd: HWND) {
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
        SetTimer(hwnd, 1, 1000, None);
        InvalidateRect(hwnd, null(), 0);
    }
    unsafe fn command(&mut self, hwnd: HWND, id: usize) {
        match id {
            101..=105 => {
                self.settings.range = id - 101;
                self.hover = None;
            }
            201..=206 => self.settings.visible[id - 201] = !self.settings.visible[id - 201],
            301 => {
                self.settings.topmost = !self.settings.topmost;
                SetWindowPos(
                    hwnd,
                    if self.settings.topmost {
                        HWND_TOPMOST
                    } else {
                        HWND_NOTOPMOST
                    },
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            302 => self.hide(hwnd),
            303 => {
                if let Err(e) = settings::set_startup(!settings::startup_enabled()) {
                    MessageBoxW(
                        hwnd,
                        wide(&e).as_ptr(),
                        wide("Performance Glass").as_ptr(),
                        MB_ICONERROR,
                    );
                }
            }
            304 => {
                DestroyWindow(hwnd);
                return;
            }
            305 => self.show(hwnd),
            306 => self.menu(hwnd),
            307 => {
                self.hover = None;
                self.drag_start = None;
                if GetCapture() == hwnd {
                    ReleaseCapture();
                }
                self.tooltip_hide(hwnd);
                let mut rect: RECT = zeroed();
                GetWindowRect(hwnd, &mut rect);
                if !self.settings.frameless {
                    self.update_position(hwnd);
                }
                self.settings.frameless = !self.settings.frameless;
                let style = crate::window_style::with_frame(
                    GetWindowLongPtrW(hwnd, GWL_STYLE) as u32,
                    self.settings.frameless,
                );
                SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE,
                );
                if !self.settings.frameless && (rect.bottom - rect.top) < self.px(280.) {
                    SetWindowPos(
                        hwnd,
                        null_mut(),
                        rect.left,
                        rect.top,
                        self.px(self.settings.normal_width as f64),
                        self.px(self.settings.normal_height as f64),
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
            308 => self.menu(hwnd),
            _ => return,
        }
        self.save_settings();
        InvalidateRect(hwnd, null(), 0);
    }
    unsafe fn menu(&mut self, hwnd: HWND) {
        self.hover = None;
        self.tooltip_hide(hwnd);
        let menu = CreatePopupMenu();
        let add = |id: usize, label: &str, checked: bool| {
            AppendMenuW(
                menu,
                MF_STRING | if checked { MF_CHECKED } else { MF_UNCHECKED },
                id,
                wide(label).as_ptr(),
            );
        };
        add(305, tr("ウィンドウを表示"), false);
        add(301, tr("常に手前に表示 (&T)"), self.settings.topmost);
        add(
            307,
            tr("グラフだけ表示 (ダブルクリック / F11)"),
            self.settings.frameless,
        );
        crate::packaged_startup::refresh();
        add(
            303,
            tr("Windowsログオン時に起動"),
            settings::startup_enabled(),
        );
        add(302, tr("トレイに格納"), false);
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        for (i, name) in range_labels().iter().enumerate() {
            add(
                101 + i,
                &i18n::message("履歴: {}", name),
                self.settings.range == i,
            );
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        for (i, name) in names().iter().enumerate() {
            if self.available[i] {
                add(
                    201 + i,
                    &i18n::message("線を表示: {}", name),
                    self.settings.visible[i],
                );
            }
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        add(304, tr("終了 (&X)"), false);
        let mut pt: POINT = zeroed();
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        let id = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            0,
            hwnd,
            null(),
        );
        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
        if id != 0 {
            self.command(hwnd, id as usize);
        }
    }
    unsafe fn text(&self, dc: HDC, text: &str, rect: RECT, font: HFONT, color: u32, flags: u32) {
        let old = SelectObject(dc, font);
        SetTextColor(dc, color);
        SetBkMode(dc, TRANSPARENT as i32);
        let value = wide(text);
        let mut r = rect;
        DrawTextW(
            dc,
            value.as_ptr(),
            (value.len() - 1) as i32,
            &mut r,
            flags | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        SelectObject(dc, old);
    }
    unsafe fn label(
        &self,
        dc: HDC,
        text: &str,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        font: HFONT,
        color: u32,
    ) {
        self.text(
            dc,
            text,
            RECT {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            },
            font,
            color,
            DT_LEFT,
        );
    }
    unsafe fn fill(&self, dc: HDC, rect: RECT, color: u32) {
        let brush = CreateSolidBrush(color);
        FillRect(dc, &rect, brush);
        DeleteObject(brush);
    }
    unsafe fn line(&self, dc: HDC, a: POINT, b: POINT, color: u32, width: i32, style: i32) {
        let pen = CreatePen(style, width, color);
        let old = SelectObject(dc, pen);
        MoveToEx(dc, a.x, a.y, null_mut());
        LineTo(dc, b.x, b.y);
        SelectObject(dc, old);
        DeleteObject(pen);
    }
}

fn format_age(s: u64) -> String {
    if s >= 3600 {
        format!("{}{} {}{}", s / 3600, tr("時間"), s % 3600 / 60, tr("分"))
    } else if s >= 60 {
        format!("{}{} {}{}", s / 60, tr("分"), s % 60, tr("秒"))
    } else {
        format!("{s}{}", tr("秒"))
    }
}
fn format_duration(s: u64) -> String {
    let value = if s >= 3600 && s % 3600 == 0 {
        format!("{}{}", s / 3600, tr("時間"))
    } else if s >= 60 && s % 60 == 0 {
        format!("{}{}", s / 60, tr("分"))
    } else {
        format_age(s)
    };
    format!("−{value}")
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = &*(l as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, w, l);
    }
    let app = &mut *ptr;
    if msg == app.taskbar_created {
        app.tray_add(hwnd);
        return 0;
    }
    match msg {
        WM_NCCALCSIZE if app.settings.frameless => 0,
        WM_NCHITTEST if app.settings.frameless => {
            let x = (l & 0xffff) as i16 as i32;
            let y = ((l >> 16) & 0xffff) as i16 as i32;
            let mut rect: RECT = zeroed();
            GetWindowRect(hwnd, &mut rect);
            let grip = app.px(7.).max(5).min((rect.bottom - rect.top) / 3);
            let left = x < rect.left + grip;
            let right = x >= rect.right - grip;
            let top = y < rect.top + grip;
            let bottom = y >= rect.bottom - grip;
            match (left, right, top, bottom) {
                (true, _, true, _) => HTTOPLEFT as isize,
                (_, true, true, _) => HTTOPRIGHT as isize,
                (true, _, _, true) => HTBOTTOMLEFT as isize,
                (_, true, _, true) => HTBOTTOMRIGHT as isize,
                (true, _, _, _) => HTLEFT as isize,
                (_, true, _, _) => HTRIGHT as isize,
                (_, _, true, _) => HTTOP as isize,
                (_, _, _, true) => HTBOTTOM as isize,
                _ => HTCLIENT as isize,
            }
        }
        WM_LBUTTONDBLCLK => {
            app.drag_start = None;
            if GetCapture() == hwnd {
                ReleaseCapture();
            }
            let x = (l & 0xffff) as i16 as i32;
            let y = ((l >> 16) & 0xffff) as i16 as i32;
            if x >= app.plot.left
                && x <= app.plot.right
                && y >= app.plot.top
                && y <= app.plot.bottom
            {
                app.command(hwnd, 307);
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut r: RECT = zeroed();
            GetClientRect(hwnd, &mut r);
            app.buffer.ensure(dc, r.right, r.bottom);
            app.render(hwnd, app.buffer.dc, r.right, r.bottom);
            app.paint_count += 1;
            BitBlt(dc, 0, 0, r.right, r.bottom, app.buffer.dc, 0, 0, SRCCOPY);
            EndPaint(hwnd, &ps);
            0
        }
        WM_GETMINMAXINFO => {
            let info = &mut *(l as *mut MINMAXINFO);
            info.ptMinTrackSize = minimum_window_size(app.scale, app.settings.frameless);
            0
        }
        WM_SIZE => {
            if w as u32 == SIZE_MINIMIZED {
                app.hide(hwnd);
            } else {
                if w as u32 == SIZE_RESTORED {
                    app.update_position(hwnd);
                }
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_DPICHANGED => {
            if !app.forced_dpi {
                app.scale = (w & 0xffff) as f64 / 96.;
            }
            let fonts = Fonts::new(app.scale);
            if !app.tooltip.is_null() {
                SendMessageW(app.tooltip, WM_SETFONT, fonts.small as usize, 0);
                SendMessageW(app.tooltip, TTM_SETMAXTIPWIDTH, 0, app.px(560.) as isize);
            }
            app.fonts = fonts;
            let r = &*(l as *const RECT);
            SetWindowPos(
                hwnd,
                null_mut(),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            0
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            app.theme(hwnd);
            0
        }
        WM_EXITSIZEMOVE => {
            app.update_position(hwnd);
            app.save_settings();
            0
        }
        WM_LBUTTONDOWN => {
            if app.settings.frameless {
                let mut p = POINT {
                    x: (l & 0xffff) as i16 as i32,
                    y: ((l >> 16) & 0xffff) as i16 as i32,
                };
                ClientToScreen(hwnd, &mut p);
                let mut rect: RECT = zeroed();
                GetWindowRect(hwnd, &mut rect);
                app.drag_start = Some(DragStart {
                    cursor: p,
                    rect,
                    moved: false,
                });
                SetCapture(hwnd);
            }
            0
        }
        WM_LBUTTONUP => {
            let was_dragging = app.drag_start.is_some_and(|drag| drag.moved);
            app.drag_start = None;
            if GetCapture() == hwnd {
                ReleaseCapture();
            }
            if was_dragging {
                app.update_position(hwnd);
                app.save_settings();
                return 0;
            }
            let x = (l & 0xffff) as i16 as i32;
            let y = ((l >> 16) & 0xffff) as i16 as i32;
            let command = app
                .hits
                .iter()
                .find(|hit| {
                    x >= hit.rect.left
                        && x < hit.rect.right
                        && y >= hit.rect.top
                        && y < hit.rect.bottom
                })
                .map(|h| h.command);
            if let Some(command) = command {
                app.command(hwnd, command);
            }
            0
        }
        WM_MOUSEMOVE => {
            if w & 0x0001 == 0 {
                app.drag_start = None;
            }
            if let Some(mut start) = app.drag_start {
                let mut pos = POINT {
                    x: (l & 0xffff) as i16 as i32,
                    y: ((l >> 16) & 0xffff) as i16 as i32,
                };
                ClientToScreen(hwnd, &mut pos);
                if start.moved
                    || (pos.x - start.cursor.x).abs() > GetSystemMetrics(SM_CXDRAG)
                    || (pos.y - start.cursor.y).abs() > GetSystemMetrics(SM_CYDRAG)
                {
                    start.moved = true;
                    app.drag_start = Some(start);
                    app.hover = None;
                    app.tooltip_hide(hwnd);
                    // Captured dragging never enters a blocking caption move loop.
                    SetWindowPos(
                        hwnd,
                        null_mut(),
                        start.rect.left + pos.x - start.cursor.x,
                        start.rect.top + pos.y - start.cursor.y,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                    return 0;
                }
            }
            let x = (l & 0xffff) as i16 as i32;
            let y = ((l >> 16) & 0xffff) as i16 as i32;
            let hover = if x >= app.plot.left
                && x <= app.plot.right
                && y >= app.plot.top
                && y <= app.plot.bottom
            {
                Some(x)
            } else {
                None
            };
            app.hover = hover;
            if let Some(x) = hover {
                app.tooltip_update(hwnd, x);
            } else {
                app.tooltip_hide(hwnd);
            }
            let mut track = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            TrackMouseEvent(&mut track);
            0
        }
        WM_MOUSELEAVE => {
            app.hover = None;
            app.tooltip_hide(hwnd);
            0
        }
        WM_CAPTURECHANGED => {
            app.drag_start = None;
            0
        }
        WM_CONTEXTMENU => {
            app.drag_start = None;
            app.tooltip_hide(hwnd);
            app.menu(hwnd);
            0
        }
        WM_KEYDOWN => {
            match w {
                0x31..=0x35 => app.command(hwnd, 101 + w - 0x31),
                0x54 => app.command(hwnd, 301),
                0x1b => {
                    if app.settings.frameless {
                        app.command(hwnd, 307);
                    } else {
                        app.hide(hwnd);
                    }
                }
                0x7a => app.command(hwnd, 307),
                0x5d | 0x79 => app.menu(hwnd),
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            app.command(hwnd, w & 0xffff);
            0
        }
        WM_CLOSE => {
            app.hide(hwnd);
            0
        }
        WM_TIMER => {
            if let Some(error) = crate::packaged_startup::take_error() {
                MessageBoxW(
                    hwnd,
                    wide(&error).as_ptr(),
                    wide("Performance Glass").as_ptr(),
                    MB_ICONERROR,
                );
            }
            if w == 1 {
                if app.benchmark_render {
                    app.render_now(hwnd);
                } else {
                    InvalidateRect(hwnd, null(), 0);
                }
                if let Some(x) = app.hover {
                    app.tooltip_update(hwnd, x);
                }
            }
            if app.test_ui && app.started.elapsed().as_secs() >= 3 {
                app.test_ui = false;
                PostMessageW(hwnd, TEST_UI, 0, 0);
            }
            if app.started.elapsed().as_secs() >= 5 {
                if let Some(path) = app.snapshot.take() {
                    app.render_now(hwnd);
                    if let Err(e) = save_bitmap(
                        app.buffer.dc,
                        app.buffer.bitmap,
                        app.buffer.width,
                        app.buffer.height,
                        &path,
                    ) {
                        app.qa_errors.push(e.to_string());
                    }
                }
            }
            if app
                .auto_exit
                .is_some_and(|seconds| app.started.elapsed().as_secs() >= seconds)
            {
                DestroyWindow(hwnd);
            }
            0
        }
        TRAY_MESSAGE => {
            match l as u32 {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                    if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
                        app.show(hwnd);
                    } else {
                        app.hide(hwnd);
                    }
                }
                WM_RBUTTONUP => app.menu(hwnd),
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            app.update_position(hwnd);
            app.save_settings();
            Shell_NotifyIconW(NIM_DELETE, &app.tray);
            KillTimer(hwnd, 1);
            KillTimer(hwnd, 2);
            if let Some(path) = &app.diagnostic {
                let elapsed = app.started.elapsed().as_secs_f64();
                let cpu = crate::process_cpu() - app.cpu_start;
                let logical = std::thread::available_parallelism()
                    .map(|v| v.get())
                    .unwrap_or(1);
                let text=format!("elapsed_s={elapsed:.3}\nprocess_cpu_s={cpu:.5}\none_core_percent={:.4}\nwhole_system_cpu_percent={:.5}\nlogical_processors={logical}\nworking_set_mib={:.3}\nsample_count={}\npaint_count={}\ngdi_objects={}\nqa_errors={}\n",cpu/elapsed*100.,cpu/elapsed*100./logical as f64,crate::working_set(),app.data.lock().unwrap().samples.len(),app.paint_count,GetGuiResources(GetCurrentProcess(),GR_GDIOBJECTS),app.qa_errors.len());
                let _ = std::fs::write(path, text);
                if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(path) {
                    use std::io::Write;
                    let _ = writeln!(
                        file,
                        "graph_seconds={:.6}\nline_seconds={:.6}\nframe_seconds={:.6}",
                        app.profile_graph_s, app.profile_lines_s, app.profile_frame_s
                    );
                }
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

pub fn run(data: Arc<Mutex<History>>, args: &[String]) -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(null());
        let test_mode = args.iter().any(|s| {
            matches!(
                s.as_str(),
                "--demo" | "--snapshot" | "--diagnostic" | "--test-ui" | "--qa-dir"
            )
        });
        let mutex_name = if test_mode {
            format!("Local\\PerformanceGlass.Native.QA.{}", std::process::id())
        } else {
            "Local\\PerformanceGlass.Native.Singleton".into()
        };
        let mutex = CreateMutexW(null(), 0, wide(&mutex_name).as_ptr());
        if mutex.is_null() {
            return Err(tr("アプリの起動ロックを作成できません。").into());
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let existing = FindWindowW(wide(CLASS).as_ptr(), null());
            if !existing.is_null() {
                ShowWindow(existing, SW_RESTORE);
                PostMessageW(existing, WM_COMMAND, 305, 0);
                SetForegroundWindow(existing);
            }
            CloseHandle(mutex);
            return Ok(());
        }
        let class_name = wide(CLASS);
        let owned_icon = make_icon(instance);
        crate::packaged_startup::refresh();
        let icon = if owned_icon.is_null() {
            LoadIconW(null_mut(), IDI_APPLICATION)
        } else {
            owned_icon
        };
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: icon,
            lpszClassName: class_name.as_ptr(),
            ..zeroed()
        };
        if RegisterClassW(&class) == 0 {
            if !owned_icon.is_null() {
                DestroyIcon(owned_icon);
            }
            CloseHandle(mutex);
            return Err(format!(
                "{} ({})",
                tr("ウィンドウを登録できません"),
                GetLastError()
            ));
        }
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES,
        });
        let arg = |name: &str| {
            args.iter()
                .position(|s| s == name)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        let mut settings = if test_mode {
            Settings::default()
        } else {
            Settings::load()
        };
        if args.iter().any(|s| s == "--frameless") {
            settings.frameless = true;
        }
        if let Some(range) = arg("--range").and_then(|s| s.parse::<usize>().ok()) {
            settings.range = range.min(4);
        }
        let scale = arg("--dpi")
            .and_then(|s| s.parse::<f64>().ok())
            .map(|dpi| dpi.clamp(96., 384.) / 96.)
            .unwrap_or_else(|| GetDpiForSystem() as f64 / 96.);
        let size = arg("--size")
            .and_then(|s| {
                s.split_once('x')
                    .map(|(w, h)| (w.parse::<i32>().ok(), h.parse::<i32>().ok()))
            })
            .and_then(|(w, h)| Some((w?, h?)));
        let mut app = Box::new(App {
            data,
            settings,
            palette: Palette::new(false),
            scale,
            fonts: Fonts::new(scale),
            buffer: BackBuffer::new(),
            strokes: Default::default(),
            hits: Vec::new(),
            available: [false; 6],
            availability_until: [0.; 6],
            chart_end: 0.,
            network_top: 0.1,
            network_range: usize::MAX,
            network_lower_since: None,
            plot: zeroed(),
            hover: None,
            tray: zeroed(),
            icon,
            taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            snapshot: arg("--snapshot"),
            test_ui: args.iter().any(|s| s == "--test-ui"),
            started: Instant::now(),
            cpu_start: crate::process_cpu(),
            diagnostic: arg("--diagnostic"),
            auto_exit: arg("--exit-after").and_then(|v| v.parse().ok()),
            demo: args.iter().any(|s| s == "--demo"),
            tooltip: null_mut(),
            tooltip_text: vec![0],
            tooltip_on: false,
            theme_updating: false,
            applied_dark: None,
            drag_start: None,
            persist: !test_mode,
            theme_override: arg("--theme").and_then(|s| match s.as_str() {
                "dark" => Some(true),
                "light" => Some(false),
                _ => None,
            }),
            forced_dpi: arg("--dpi").is_some(),
            benchmark_render: args.iter().any(|s| s == "--benchmark-render"),
            paint_count: 0,
            profile_graph_s: 0.,
            profile_lines_s: 0.,
            profile_frame_s: 0.,
            qa_dir: arg("--qa-dir"),
            qa_errors: Vec::new(),
        });
        let initial_size = size.unwrap_or_else(|| {
            (
                app.px(app.settings.width as f64),
                app.px(app.settings.height as f64),
            )
        });
        let minimum = minimum_window_size(scale, app.settings.frameless);
        let initial_size = (initial_size.0.max(minimum.x), initial_size.1.max(minimum.y));
        let mut initial = RECT {
            left: app.settings.x,
            top: app.settings.y,
            right: app.settings.x.saturating_add(initial_size.0),
            bottom: app.settings.y.saturating_add(initial_size.1),
        };
        if app.settings.x != i32::MIN && app.settings.y != i32::MIN {
            let monitor = MonitorFromRect(&initial, MONITOR_DEFAULTTONEAREST);
            let mut info: MONITORINFO = zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                initial.left = initial
                    .left
                    .clamp(info.rcWork.left, info.rcWork.right - 140);
                initial.top = initial.top.clamp(info.rcWork.top, info.rcWork.bottom - 40);
            }
        }
        let hwnd = CreateWindowExW(
            0,
            wide(CLASS).as_ptr(),
            wide("Performance Glass").as_ptr(),
            if app.settings.frameless {
                crate::window_style::with_frame(0, true)
            } else {
                WS_OVERLAPPEDWINDOW
            },
            initial.left,
            initial.top,
            initial_size.0,
            initial_size.1,
            null_mut(),
            null_mut(),
            instance,
            &mut *app as *mut App as _,
        );
        if hwnd.is_null() {
            if !owned_icon.is_null() {
                DestroyIcon(owned_icon);
            }
            CloseHandle(mutex);
            return Err(format!(
                "{} ({})",
                tr("ウィンドウを作成できません"),
                GetLastError()
            ));
        }
        app.create_tooltip(hwnd);
        app.theme(hwnd);
        let tray_ok = app.tray_add(hwnd);
        if app.settings.topmost {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        if !args.iter().any(|s| s == "--tray") || !tray_ok {
            ShowWindow(hwnd, SW_SHOW);
            SetTimer(hwnd, 1, 1000, None);
        }
        if app.auto_exit.is_some() || app.test_ui || app.snapshot.is_some() {
            SetTimer(hwnd, 2, 1000, None);
        }
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if msg.message == TEST_UI {
                verification::run(hwnd, &mut *app as *mut App);
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        CloseHandle(mutex);
        if !owned_icon.is_null() {
            DestroyIcon(owned_icon);
        }
        Ok(())
    }
}

unsafe fn make_icon(instance: HINSTANCE) -> HICON {
    // Load an owned handle from the embedded, multi-resolution icon resource.
    LoadImageW(
        instance,
        101_usize as *const u16,
        IMAGE_ICON,
        0,
        0,
        LR_DEFAULTSIZE,
    ) as HICON
}

unsafe fn save_bitmap(
    dc: HDC,
    bitmap: HBITMAP,
    width: i32,
    height: i32,
    path: &str,
) -> std::io::Result<()> {
    use std::io::Write;
    if width <= 0 || height <= 0 {
        return Ok(());
    }
    let mut info: BITMAPINFO = zeroed();
    info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    let mut pixels = vec![0_u8; (width * height * 4) as usize];
    // GetDIBits requires the bitmap not to be selected in a DC.
    let temporary = CreateBitmap(1, 1, 1, 1, null());
    let old = SelectObject(dc, temporary);
    let result = GetDIBits(
        dc,
        bitmap,
        0,
        height as u32,
        pixels.as_mut_ptr() as _,
        &mut info,
        DIB_RGB_COLORS,
    );
    SelectObject(dc, old);
    DeleteObject(temporary);
    if result == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut f = std::fs::File::create(path)?;
    f.write_all(b"BM")?;
    f.write_all(&(54 + pixels.len() as u32).to_le_bytes())?;
    f.write_all(&[0_u8; 4])?;
    f.write_all(&54_u32.to_le_bytes())?;
    let header = std::slice::from_raw_parts(&info.bmiHeader as *const _ as *const u8, 40);
    f.write_all(header)?;
    f.write_all(&pixels)
}
