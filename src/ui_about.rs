use super::*;

const DIALOG: usize = 102;
const NAME: i32 = 1001;
const VERSION: i32 = 1002;
const GITHUB: i32 = 1003;
const STORE: i32 = 1004;

const GITHUB_URL: &str = "https://github.com/nekomimiyks";
const STORE_APP: &str = "ms-windows-store://publisher/?name=nekomimiyks";
const STORE_WEB: &str = "https://apps.microsoft.com/search/publisher?name=nekomimiyks";

struct Colors {
    dark: i32,
    background: HBRUSH,
    surface: u32,
    text: u32,
    muted: u32,
    border: u32,
    link: u32,
}

pub(super) unsafe fn show(owner: HWND, dark: bool) {
    let [background, surface, text, muted, border, link] = if dark {
        [0x202424, 0x282d2d, 0xeef3f0, 0xb2c0b9, 0x424b47, 0x8ad5c1]
    } else {
        [0xf4f6f5, 0xffffff, 0x202825, 0x51605a, 0xd4dcd8, 0x07675a]
    }
    .map(hex);
    let colors = Colors {
        dark: i32::from(dark),
        background: CreateSolidBrush(background),
        surface,
        text,
        muted,
        border,
        link,
    };
    // DialogBoxParamW is modal. The color palette lives through every callback.
    let dialog = DialogBoxParamW(
        GetModuleHandleW(null()),
        DIALOG as *const u16,
        owner,
        Some(dialog_proc),
        &colors as *const Colors as LPARAM,
    );
    DeleteObject(colors.background);
    if dialog == -1 {
        MessageBoxW(
            owner,
            wide(&version()).as_ptr(),
            wide("Performance Glass").as_ptr(),
            MB_OK,
        );
    }
}

fn version() -> String {
    i18n::message("バージョン: {}", env!("CARGO_PKG_VERSION"))
}

fn open_store(mut open: impl FnMut(&str) -> bool) -> bool {
    open(STORE_APP) || open(STORE_WEB)
}

unsafe extern "system" fn dialog_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> isize {
    if msg == WM_INITDIALOG {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, l);
        SetWindowTextW(hwnd, wide(tr("アプリについて")).as_ptr());
        for (id, text) in [
            (NAME, "Performance Glass"),
            (VERSION, &version()),
            (GITHUB, tr("GitHubの公開アプリ")),
            (STORE, tr("Microsoft Storeの公開アプリ")),
            (IDCANCEL, tr("閉じる")),
        ] {
            SetDlgItemTextW(hwnd, id, wide(text).as_ptr());
        }
        // Match the Windows title bar while deriving the content palette from
        // the parent window's app theme.
        let colors = &*(l as *const Colors);
        DwmSetWindowAttribute(hwnd, 20, &colors.dark as *const _ as _, 4);
        SetFocus(GetDlgItem(hwnd, IDCANCEL));
        return 0;
    }
    let colors = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Colors;
    let Some(colors) = colors.as_ref() else {
        return 0;
    };
    match msg {
        WM_CTLCOLORDLG => colors.background as isize,
        WM_CTLCOLORSTATIC => {
            let color = if GetDlgCtrlID(l as HWND) == VERSION {
                colors.muted
            } else {
                colors.text
            };
            SetTextColor(w as HDC, color);
            SetBkMode(w as HDC, TRANSPARENT as i32);
            colors.background as isize
        }
        WM_DRAWITEM => {
            draw_button(&*(l as *const DRAWITEMSTRUCT), colors);
            1
        }
        WM_SETCURSOR if matches!(GetDlgCtrlID(w as HWND), GITHUB | STORE) => {
            SetCursor(LoadCursorW(null_mut(), IDC_HAND));
            SetWindowLongPtrW(hwnd, DWLP_MSGRESULT as i32, 1);
            1
        }
        WM_CLOSE => {
            EndDialog(hwnd, 0);
            1
        }
        WM_COMMAND if w >> 16 == BN_CLICKED as usize => {
            let open = |url: &str| {
                ShellExecuteW(
                    hwnd,
                    null(),
                    wide(url).as_ptr(),
                    null(),
                    null(),
                    SW_SHOWNORMAL,
                ) as isize
                    > 32
            };
            let opened = match (w & 0xffff) as i32 {
                GITHUB => open(GITHUB_URL),
                STORE => open_store(open),
                IDOK | IDCANCEL => {
                    EndDialog(hwnd, 0);
                    return 1;
                }
                _ => return 0,
            };
            if !opened {
                MessageBoxW(
                    hwnd,
                    wide(tr("リンクを開けませんでした。")).as_ptr(),
                    wide(tr("アプリについて")).as_ptr(),
                    MB_ICONERROR,
                );
            }
            1
        }
        _ => 0,
    }
}

unsafe fn draw_button(item: &DRAWITEMSTRUCT, colors: &Colors) {
    let close = item.CtlID == IDCANCEL as u32;
    let pressed = item.itemState & ODS_SELECTED != 0;
    let mut rect = item.rcItem;
    if close {
        let brush = CreateSolidBrush(if pressed {
            colors.border
        } else {
            colors.surface
        });
        FillRect(item.hDC, &rect, brush);
        DeleteObject(brush);
        let border = CreateSolidBrush(colors.border);
        FrameRect(item.hDC, &rect, border);
        DeleteObject(border);
    } else {
        FillRect(item.hDC, &rect, colors.background);
    }
    let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
    let previous_font = SelectObject(item.hDC, font);
    let length = GetWindowTextLengthW(item.hwndItem);
    let mut text = vec![0; length as usize + 1];
    GetWindowTextW(item.hwndItem, text.as_mut_ptr(), length + 1);
    SetBkMode(item.hDC, TRANSPARENT as i32);
    SetTextColor(item.hDC, if close { colors.text } else { colors.link });
    DrawTextW(
        item.hDC,
        text.as_ptr(),
        length,
        &mut rect,
        DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | if close { DT_CENTER } else { DT_LEFT },
    );
    SelectObject(item.hDC, previous_font);
    if item.itemState & ODS_FOCUS != 0 && item.itemState & ODS_NOFOCUSRECT == 0 {
        InflateRect(&mut rect, -2, -2);
        DrawFocusRect(item.hDC, &rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tries_browser_only_when_store_launch_fails() {
        for (accept, expected) in [
            (Some(STORE_APP), &[STORE_APP][..]),
            (Some(STORE_WEB), &[STORE_APP, STORE_WEB][..]),
            (None, &[STORE_APP, STORE_WEB][..]),
        ] {
            let mut attempts = Vec::new();
            let opened = open_store(|url| {
                attempts.push(url.to_owned());
                Some(url) == accept
            });
            assert_eq!(attempts, expected);
            assert_eq!(opened, accept.is_some());
        }
    }
}
