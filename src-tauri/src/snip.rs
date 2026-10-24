//! Keep the native selection UI in control until the final clipboard image is ready.
pub const RECTANGLE_URI: &str = "ms-screenclip:?source=Darkroom&clippingMode=Rectangle";
const SETTLE_MS: u64 = 600;

pub struct CaptureGate {
    observed: u32,
    consumed: u32,
    stable_since: u64,
}
impl CaptureGate {
    pub fn new(sequence: u32, now: u64) -> Self {
        Self {
            observed: sequence,
            consumed: sequence,
            stable_since: now,
        }
    }
    pub fn ready(&mut self, sequence: u32, selecting: bool, now: u64) -> bool {
        if sequence != self.observed {
            self.observed = sequence;
            self.stable_since = now;
        }
        if selecting {
            self.stable_since = now;
            return false;
        }
        sequence != self.consumed && now.saturating_sub(self.stable_since) >= SETTLE_MS
    }
    pub fn consume(&mut self, sequence: u32) {
        self.consumed = sequence;
    }
    pub fn reset(&mut self, sequence: u32, now: u64) {
        *self = Self::new(sequence, now);
    }
}

pub fn selection_active() -> bool {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
            WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId},
        },
    };
    use windows::core::PWSTR;
    unsafe {
        // In particular, do not activate our window while a drag is still held.
        if GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 {
            return true;
        }
        let foreground = GetForegroundWindow();
        if foreground.0.is_null() {
            return false;
        }
        let mut class = [0u16; 256];
        let length = GetClassNameW(foreground, &mut class).max(0) as usize;
        let class = String::from_utf16_lossy(&class[..length]).to_ascii_lowercase();
        if class.contains("screenclipping")
            || class.contains("snippercapture")
            || class.contains("snippingtoolcapture")
        {
            return true;
        }
        let mut process_id = 0;
        GetWindowThreadProcessId(foreground, Some(&mut process_id));
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) else {
            return false;
        };
        let mut path = vec![0u16; 1024];
        let mut size = path.len() as u32;
        let queried = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(process);
        queried.is_ok()
            && String::from_utf16_lossy(&path[..size as usize])
                .to_ascii_lowercase()
                .ends_with("\\screenclippinghost.exe")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_existing_clipboard_and_cancel_without_new_image() {
        let mut gate = CaptureGate::new(10, 0);
        assert!(!gate.ready(10, false, 2000));
        assert!(!gate.ready(10, true, 2200));
        assert!(!gate.ready(10, false, 5000));
    }
    #[test]
    fn never_pops_up_during_selection_or_mouse_drag() {
        let mut gate = CaptureGate::new(10, 0);
        assert!(!gate.ready(11, true, 100));
        assert!(!gate.ready(11, true, 3000));
        assert!(!gate.ready(11, false, 3100));
        assert!(gate.ready(11, false, 3600));
    }
    #[test]
    fn waits_for_final_region_instead_of_intermediate_fullscreen_image() {
        let mut gate = CaptureGate::new(10, 0);
        assert!(!gate.ready(11, true, 100));
        assert!(!gate.ready(12, true, 2000));
        assert!(!gate.ready(12, false, 2200));
        assert!(gate.ready(12, false, 2600));
        gate.consume(12);
        assert!(!gate.ready(12, false, 4000));
    }
    #[test]
    fn new_clipboard_image_restarts_settle_window() {
        let mut gate = CaptureGate::new(10, 0);
        assert!(!gate.ready(11, false, 100));
        assert!(!gate.ready(12, false, 600));
        assert!(!gate.ready(12, false, 900));
        assert!(gate.ready(12, false, 1200));
        gate.reset(13, 1400);
        assert!(!gate.ready(13, false, 5000));
    }
    #[test]
    fn launch_explicitly_requests_rectangular_selection() {
        assert!(RECTANGLE_URI.contains("clippingMode=Rectangle"));
        assert!(!RECTANGLE_URI.contains("type=snapshot"));
    }
}
