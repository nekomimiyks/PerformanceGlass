use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Change only the frame/popup bits. WS_VISIBLE and all unrelated runtime state
/// belong to Windows and must survive a normal <-> graph-only transition.
pub fn with_frame(current: u32, frameless: bool) -> u32 {
    let frame_bits = WS_OVERLAPPEDWINDOW | WS_POPUP;
    (current & !frame_bits)
        | if frameless {
            WS_POPUP | WS_THICKFRAME | WS_SYSMENU
        } else {
            WS_OVERLAPPEDWINDOW
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_visible_window_stays_visible_after_both_transitions() {
        let original = WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPSIBLINGS;
        let graph = with_frame(original, true);
        assert_ne!(
            graph & WS_VISIBLE,
            0,
            "graph-only toggle must preserve WS_VISIBLE"
        );
        assert_eq!(graph & WS_CAPTION, 0);
        assert_eq!(graph & WS_CLIPSIBLINGS, original & WS_CLIPSIBLINGS);
        let normal = with_frame(graph, false);
        assert_eq!(normal, original);
    }
    #[test]
    fn hidden_window_and_unrelated_state_are_preserved() {
        for unrelated in [0, WS_DISABLED, WS_MAXIMIZE, WS_MINIMIZE, WS_CLIPCHILDREN] {
            let original = WS_OVERLAPPEDWINDOW | unrelated;
            let graph = with_frame(original, true);
            assert_eq!(graph & WS_VISIBLE, 0);
            assert_eq!(graph & unrelated, unrelated);
            assert_eq!(with_frame(graph, false), original);
        }
    }
}
