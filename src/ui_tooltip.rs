use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) struct TooltipStyle {
    pub font: HFONT,
    pub background: u32,
    pub foreground: u32,
    pub border: u32,
    pub scale: f64,
}

pub(super) struct Tooltip {
    pub hwnd: HWND,
    buffer: BackBuffer,
    text: String,
    style: Option<TooltipStyle>,
    work: (i32, i32, i32, i32),
    position: Option<(i32, i32)>,
    size: SIZE,
}

impl Tooltip {
    pub fn new() -> Self {
        Self {
            hwnd: null_mut(),
            buffer: BackBuffer::new(),
            text: String::new(),
            style: None,
            work: (0, 0, 0, 0),
            position: None,
            size: SIZE { cx: 0, cy: 0 },
        }
    }

    pub unsafe fn create(&mut self, owner: HWND) {
        self.hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            wide("STATIC").as_ptr(),
            null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            owner,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
    }

    pub unsafe fn hide(&mut self) {
        if self.position.take().is_some() {
            ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    pub unsafe fn update(&mut self, text: String, cursor: POINT, style: TooltipStyle) {
        if self.hwnd.is_null() {
            return;
        }
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return;
        }
        let r = info.rcWork;
        let work = (r.left, r.top, r.right, r.bottom);
        let redraw = self.text != text || self.style != Some(style) || self.work != work;
        let px = |n: f64| (n * style.scale).round() as i32;
        let mut value = Vec::new();
        if redraw {
            value = wide(&text);
            let padding = px(6.).max(2);
            let screen = GetDC(null_mut());
            if screen.is_null() {
                return;
            }
            self.buffer
                .ensure(screen, self.buffer.width.max(1), self.buffer.height.max(1));
            let old_font = SelectObject(self.buffer.dc, style.font);
            let flags = DT_NOPREFIX | DT_WORDBREAK | DT_EDITCONTROL;
            let mut extent = RECT {
                left: 0,
                top: 0,
                right: (px(560.).min(r.right - r.left) - 2 * padding).max(1),
                bottom: 0,
            };
            let length = (value.len() - 1) as i32;
            DrawTextW(
                self.buffer.dc,
                value.as_ptr(),
                length,
                &mut extent,
                flags | DT_CALCRECT,
            );
            SelectObject(self.buffer.dc, old_font);
            self.size = SIZE {
                cx: (extent.right + 2 * padding).min(r.right - r.left).max(1),
                cy: (extent.bottom + 2 * padding).min(r.bottom - r.top).max(1),
            };
            self.buffer.ensure(
                screen,
                self.buffer.width.max(self.size.cx),
                self.buffer.height.max(self.size.cy),
            );
            ReleaseDC(null_mut(), screen);
            let outer = RECT {
                left: 0,
                top: 0,
                right: self.size.cx,
                bottom: self.size.cy,
            };
            let background = CreateSolidBrush(style.background);
            let border = CreateSolidBrush(style.border);
            FillRect(self.buffer.dc, &outer, background);
            FrameRect(self.buffer.dc, &outer, border);
            DeleteObject(background);
            DeleteObject(border);
            let mut inner = RECT {
                left: padding,
                top: padding,
                right: outer.right - padding,
                bottom: outer.bottom - padding,
            };
            let old_font = SelectObject(self.buffer.dc, style.font);
            SetBkMode(self.buffer.dc, TRANSPARENT as i32);
            SetTextColor(self.buffer.dc, style.foreground);
            DrawTextW(self.buffer.dc, value.as_ptr(), length, &mut inner, flags);
            SelectObject(self.buffer.dc, old_font);
        }
        let gap_x = px(12.);
        let gap_y = px(16.);
        let x = cursor.x + gap_x;
        let mut y = cursor.y + gap_y;
        if y + self.size.cy > r.bottom {
            y = cursor.y - gap_y - self.size.cy;
        }
        let destination = POINT {
            x: x.clamp(r.left, (r.right - self.size.cx).max(r.left)),
            y: y.clamp(r.top, (r.bottom - self.size.cy).max(r.top)),
        };
        let position = Some((destination.x, destination.y));
        if !redraw && self.position == position {
            return;
        }
        let source = POINT { x: 0, y: 0 };
        // Hand over the complete image and its new position in a single composition.
        if UpdateLayeredWindow(
            self.hwnd,
            null_mut(),
            &destination,
            &self.size,
            self.buffer.dc,
            &source,
            0,
            null(),
            ULW_OPAQUE,
        ) == 0
        {
            return;
        }
        let first_show = self.position.is_none();
        self.position = position;
        if redraw {
            SetWindowTextW(self.hwnd, value.as_ptr());
            self.text = text;
            self.style = Some(style);
            self.work = work;
        }
        if first_show {
            ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
    }
}
