use anyhow::Result;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub enum InjectionOutcome {
    Inserted,
    CopiedToClipboard,
}

// =============================================================================
// Windows implementation
// =============================================================================

#[cfg(windows)]
mod platform {
    use super::*;
    use std::thread;
    use std::time::Duration;

    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{
        GlobalAlloc, GlobalFree, GlobalLock, GlobalUnlock, GLOBAL_ALLOC_FLAGS,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    /// CF_UNICODETEXT = 13
    const CF_UNICODETEXT: u32 = 13;
    /// GMEM_MOVEABLE = 0x0002
    const GMEM_MOVEABLE: GLOBAL_ALLOC_FLAGS = GLOBAL_ALLOC_FLAGS(0x0002);

    const VK_CONTROL: u16 = 0x11;
    const VK_V: u16 = 0x56;

    pub struct TextInjectionEngine;

    impl TextInjectionEngine {
        pub fn new() -> Self {
            Self
        }

        pub fn inject(&self, text: &str) -> Result<InjectionOutcome> {
            if text.is_empty() {
                return Ok(InjectionOutcome::Inserted);
            }

            // Check if there's a foreground window to paste into
            let fg = unsafe { GetForegroundWindow() };
            if fg == HWND::default() {
                tracing::info!("no foreground window, copying to clipboard only");
                self.copy_to_clipboard(text)?;
                return Ok(InjectionOutcome::CopiedToClipboard);
            }

            // 1. Save current clipboard
            let saved = self.get_clipboard_text();

            // 2. Set clipboard to our text
            self.set_clipboard_text(text)?;

            // 3. Brief delay for clipboard to settle
            thread::sleep(Duration::from_millis(30));

            // 4. Simulate Ctrl+V
            self.send_paste()?;

            // 5. Wait for the paste to be consumed
            thread::sleep(Duration::from_millis(100));

            // 6. Restore original clipboard (best effort)
            if let Some(original) = saved {
                if let Err(e) = self.set_clipboard_text(&original) {
                    tracing::warn!("failed to restore clipboard: {}", e);
                }
            }

            tracing::info!("text injected ({} chars)", text.len());
            Ok(InjectionOutcome::Inserted)
        }

        pub fn copy_to_clipboard(&self, text: &str) -> Result<()> {
            self.set_clipboard_text(text)?;
            tracing::info!("copied {} chars to clipboard", text.len());
            Ok(())
        }

        fn get_clipboard_text(&self) -> Option<String> {
            unsafe {
                if OpenClipboard(HWND::default()).is_err() {
                    return None;
                }

                let result = (|| -> Option<String> {
                    let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
                    if handle.0.is_null() {
                        return None;
                    }

                    let ptr = GlobalLock(std::mem::transmute(handle.0)) as *const u16;
                    if ptr.is_null() {
                        return None;
                    }

                    // Find null terminator
                    let mut len = 0usize;
                    while *ptr.add(len) != 0 {
                        len += 1;
                    }

                    let slice = std::slice::from_raw_parts(ptr, len);
                    let text = String::from_utf16_lossy(slice);

                    GlobalUnlock(std::mem::transmute(handle.0)).ok();
                    Some(text)
                })();

                let _ = CloseClipboard();
                result
            }
        }

        fn set_clipboard_text(&self, text: &str) -> Result<()> {
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let byte_len = wide.len() * std::mem::size_of::<u16>();

            unsafe {
                // Allocate global memory
                let hmem = GlobalAlloc(GMEM_MOVEABLE, byte_len)?;

                let ptr = GlobalLock(hmem) as *mut u16;
                if ptr.is_null() {
                    GlobalFree(hmem)?;
                    anyhow::bail!("GlobalLock returned null");
                }

                std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
                GlobalUnlock(hmem).ok();

                // Open clipboard
                OpenClipboard(HWND::default())
                    .map_err(|e| anyhow::anyhow!("OpenClipboard failed: {}", e))?;

                EmptyClipboard()
                    .map_err(|e| anyhow::anyhow!("EmptyClipboard failed: {}", e))?;

                let result = SetClipboardData(CF_UNICODETEXT, windows::Win32::Foundation::HANDLE(hmem.0));
                let _ = CloseClipboard();

                result.map_err(|e| anyhow::anyhow!("SetClipboardData failed: {}", e))?;
            }

            Ok(())
        }

        fn send_paste(&self) -> Result<()> {
            let inputs = [
                // Ctrl down
                make_key_input(VK_CONTROL, false),
                // V down
                make_key_input(VK_V, false),
                // V up
                make_key_input(VK_V, true),
                // Ctrl up
                make_key_input(VK_CONTROL, true),
            ];

            unsafe {
                let sent = SendInput(
                    &inputs,
                    std::mem::size_of::<INPUT>() as i32,
                );
                if sent != inputs.len() as u32 {
                    anyhow::bail!(
                        "SendInput: expected {} events, sent {}",
                        inputs.len(),
                        sent
                    );
                }
            }

            Ok(())
        }
    }

    fn make_key_input(vk: u16, key_up: bool) -> INPUT {
        let flags = if key_up {
            KEYEVENTF_KEYUP
        } else {
            KEYBD_EVENT_FLAGS::default()
        };

        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }
}

// =============================================================================
// Non-Windows stub
// =============================================================================

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub struct TextInjectionEngine;

    impl TextInjectionEngine {
        pub fn new() -> Self {
            Self
        }

        pub fn inject(&self, text: &str) -> Result<InjectionOutcome> {
            tracing::info!("[stub] inject text: {}", text);
            Ok(InjectionOutcome::Inserted)
        }

        pub fn copy_to_clipboard(&self, text: &str) -> Result<()> {
            tracing::info!("[stub] copy to clipboard: {}", text);
            Ok(())
        }
    }
}

pub use platform::TextInjectionEngine;
