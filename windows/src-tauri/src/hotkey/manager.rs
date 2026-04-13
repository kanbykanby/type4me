use anyhow::Result;
use tokio::sync::mpsc;

use crate::app_state::ProcessingMode;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum HotkeyEvent {
    StartRecording { mode_id: String },
    StopRecording,
}

/// Commands sent from the main thread to the hotkey message-pump thread.
#[allow(dead_code)]
enum ThreadCommand {
    Register(Vec<ProcessingMode>),
    UnregisterAll,
    Shutdown,
}

// =============================================================================
// Windows implementation
// =============================================================================

#[cfg(windows)]
mod platform {
    use super::*;
    use std::sync::mpsc as std_mpsc;
    use std::thread;

    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GetMessageW, PostMessageW, PostQuitMessage, RegisterClassW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WINDOW_EX_STYLE,
        WINDOW_STYLE, WH_KEYBOARD_LL, WM_APP, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
        WM_SYSKEYUP, WNDCLASSW,
    };
    use windows::core::PCWSTR;

    /// Custom messages posted to our hidden window
    const WM_REGISTER: u32 = WM_APP + 1;
    const WM_UNREGISTER_ALL: u32 = WM_APP + 2;
    const WM_SHUTDOWN: u32 = WM_APP + 3;

    /// State for a registered hold-mode hotkey
    #[derive(Clone)]
    struct HoldKeyBinding {
        mode_id: String,
        vk: u32,
        modifiers: u32, // MOD_CONTROL=0x0002, MOD_ALT=0x0001, MOD_SHIFT=0x0004
    }

    /// Thread-local state (only accessed from the hotkey thread)
    struct HotkeyThreadState {
        hwnd: HWND,
        event_tx: mpsc::UnboundedSender<HotkeyEvent>,
        /// Toggle-mode registrations: hotkey_id -> mode_id, plus tracking whether recording
        toggle_modes: Vec<(i32, String)>,
        toggle_active: Option<String>, // mode_id currently recording, if any
        /// Hold-mode registrations tracked via the LL hook
        hold_bindings: Vec<HoldKeyBinding>,
        hold_active: bool,
        /// Next hotkey ID for RegisterHotKey (must be unique per hwnd)
        next_hotkey_id: i32,
        /// The LL keyboard hook handle (installed when hold bindings exist)
        hook: Option<HHOOK>,
    }

    // Thread-local pointer for the LL hook callback (SetWindowsHookEx requires a static fn)
    thread_local! {
        static THREAD_STATE: std::cell::RefCell<Option<*mut HotkeyThreadState>> =
            const { std::cell::RefCell::new(None) };
    }

    pub struct HotkeyManager {
        cmd_tx: std_mpsc::Sender<ThreadCommand>,
        thread: Option<thread::JoinHandle<()>>,
        /// HWND of the hidden window (for PostMessage from other threads)
        hwnd: HWND,
    }

    // HWND is a pointer wrapper that is Send-safe for our usage pattern
    // (we only PostMessage to it, which is thread-safe in Win32)
    unsafe impl Send for HotkeyManager {}
    unsafe impl Sync for HotkeyManager {}

    impl HotkeyManager {
        pub fn new() -> Result<(Self, mpsc::UnboundedReceiver<HotkeyEvent>)> {
            let (event_tx, event_rx) = mpsc::unbounded_channel::<HotkeyEvent>();
            let (cmd_tx, cmd_rx) = std_mpsc::channel::<ThreadCommand>();
            // We need to get the HWND back from the thread
            let (hwnd_tx, hwnd_rx) = std_mpsc::channel::<HWND>();

            let thread = thread::Builder::new()
                .name("hotkey-pump".into())
                .spawn(move || {
                    if let Err(e) = run_hotkey_thread(event_tx, cmd_rx, hwnd_tx) {
                        tracing::error!("hotkey thread exited with error: {}", e);
                    }
                })?;

            let hwnd = hwnd_rx
                .recv()
                .map_err(|_| anyhow::anyhow!("hotkey thread failed to send HWND"))?;

            tracing::info!("hotkey manager initialized");

            Ok((
                Self {
                    cmd_tx,
                    thread: Some(thread),
                    hwnd,
                },
                event_rx,
            ))
        }

        pub fn register(&self, modes: &[ProcessingMode]) -> Result<()> {
            self.cmd_tx
                .send(ThreadCommand::Register(modes.to_vec()))
                .map_err(|_| anyhow::anyhow!("hotkey thread gone"))?;
            // Poke the message loop
            unsafe {
                let _ = PostMessageW(self.hwnd, WM_REGISTER, WPARAM(0), LPARAM(0));
            }
            Ok(())
        }

        pub fn unregister_all(&self) -> Result<()> {
            self.cmd_tx
                .send(ThreadCommand::UnregisterAll)
                .map_err(|_| anyhow::anyhow!("hotkey thread gone"))?;
            unsafe {
                let _ = PostMessageW(self.hwnd, WM_UNREGISTER_ALL, WPARAM(0), LPARAM(0));
            }
            Ok(())
        }

        pub fn shutdown(&self) {
            let _ = self.cmd_tx.send(ThreadCommand::Shutdown);
            unsafe {
                let _ = PostMessageW(self.hwnd, WM_SHUTDOWN, WPARAM(0), LPARAM(0));
            }
        }
    }

    impl Drop for HotkeyManager {
        fn drop(&mut self) {
            self.shutdown();
            if let Some(handle) = self.thread.take() {
                let _ = handle.join();
            }
        }
    }

    /// Main function for the hotkey thread. Creates a hidden window, installs hooks,
    /// and runs the Win32 message pump.
    fn run_hotkey_thread(
        event_tx: mpsc::UnboundedSender<HotkeyEvent>,
        cmd_rx: std_mpsc::Receiver<ThreadCommand>,
        hwnd_tx: std_mpsc::Sender<HWND>,
    ) -> Result<()> {
        unsafe {
            // Register a window class
            let class_name: Vec<u16> = "Type4MeHotkeyClass\0".encode_utf16().collect();
            let wc = WNDCLASSW {
                lpfnWndProc: Some(hotkey_wnd_proc),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            let atom = RegisterClassW(&wc);
            if atom == 0 {
                anyhow::bail!("RegisterClassW failed");
            }

            // Create a message-only window (HWND_MESSAGE parent)
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(class_name.as_ptr()),
                PCWSTR::null(),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                // HWND_MESSAGE = -3
                HWND(-3isize as *mut _),
                None,
                None,
                None,
            )?;

            // Send HWND back to constructor
            hwnd_tx
                .send(hwnd)
                .map_err(|_| anyhow::anyhow!("failed to send HWND back"))?;

            // Set up thread state
            let mut state = HotkeyThreadState {
                hwnd,
                event_tx,
                toggle_modes: Vec::new(),
                toggle_active: None,
                hold_bindings: Vec::new(),
                hold_active: false,
                next_hotkey_id: 1,
                hook: None,
            };

            let state_ptr = &mut state as *mut HotkeyThreadState;
            THREAD_STATE.with(|cell| {
                *cell.borrow_mut() = Some(state_ptr);
            });

            // Message pump
            let mut msg = MSG::default();
            loop {
                let ret = GetMessageW(&mut msg, HWND::default(), 0, 0);
                if ret.0 <= 0 {
                    break; // WM_QUIT or error
                }

                // Process our custom commands from the channel
                while let Ok(cmd) = cmd_rx.try_recv() {
                    match cmd {
                        ThreadCommand::Register(modes) => {
                            handle_register(&mut state, &modes);
                        }
                        ThreadCommand::UnregisterAll => {
                            handle_unregister_all(&mut state);
                        }
                        ThreadCommand::Shutdown => {
                            handle_unregister_all(&mut state);
                            DestroyWindow(hwnd).ok();
                            PostQuitMessage(0);
                        }
                    }
                }

                // Handle WM_HOTKEY for toggle mode
                if msg.message == WM_HOTKEY {
                    let hotkey_id = msg.wParam.0 as i32;
                    handle_toggle_hotkey(&mut state, hotkey_id);
                }

                // Handle our custom messages (just drain the cmd_rx above)
                if msg.message == WM_SHUTDOWN {
                    // Already handled above via cmd_rx, but process any remaining
                    while let Ok(cmd) = cmd_rx.try_recv() {
                        match cmd {
                            ThreadCommand::Shutdown => {}
                            ThreadCommand::Register(modes) => {
                                handle_register(&mut state, &modes);
                            }
                            ThreadCommand::UnregisterAll => {
                                handle_unregister_all(&mut state);
                            }
                        }
                    }
                    handle_unregister_all(&mut state);
                    DestroyWindow(hwnd).ok();
                    PostQuitMessage(0);
                }

                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            // Cleanup thread-local
            THREAD_STATE.with(|cell| {
                *cell.borrow_mut() = None;
            });
        }

        Ok(())
    }

    fn handle_register(state: &mut HotkeyThreadState, modes: &[ProcessingMode]) {
        // Unregister existing first
        handle_unregister_all(state);

        for mode in modes {
            let (vk, mods) = match (mode.hotkey_vk, mode.hotkey_modifiers) {
                (Some(vk), Some(mods)) => (vk, mods),
                _ => continue, // No hotkey configured
            };

            match mode.hotkey_style {
                HotkeyStyle::Toggle => {
                    let id = state.next_hotkey_id;
                    state.next_hotkey_id += 1;
                    unsafe {
                        if RegisterHotKey(
                            state.hwnd,
                            id,
                            HOT_KEY_MODIFIERS(mods),
                            vk,
                        )
                        .is_ok()
                        {
                            tracing::info!(
                                "registered toggle hotkey id={} vk=0x{:X} mods=0x{:X} for mode '{}'",
                                id, vk, mods, mode.id
                            );
                            state.toggle_modes.push((id, mode.id.clone()));
                        } else {
                            tracing::error!(
                                "RegisterHotKey failed for mode '{}' (vk=0x{:X}, mods=0x{:X})",
                                mode.id, vk, mods
                            );
                        }
                    }
                }
                HotkeyStyle::Hold => {
                    state.hold_bindings.push(HoldKeyBinding {
                        mode_id: mode.id.clone(),
                        vk,
                        modifiers: mods,
                    });
                    tracing::info!(
                        "registered hold hotkey vk=0x{:X} mods=0x{:X} for mode '{}'",
                        vk, mods, mode.id
                    );
                }
            }
        }

        // Install LL keyboard hook if we have hold bindings
        if !state.hold_bindings.is_empty() && state.hook.is_none() {
            unsafe {
                match SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), None, 0) {
                    Ok(hook) => {
                        state.hook = Some(hook);
                        tracing::info!("installed low-level keyboard hook for hold mode");
                    }
                    Err(e) => {
                        tracing::error!("SetWindowsHookExW failed: {}", e);
                    }
                }
            }
        }
    }

    fn handle_unregister_all(state: &mut HotkeyThreadState) {
        // Unregister toggle hotkeys
        for (id, mode_id) in state.toggle_modes.drain(..) {
            unsafe {
                let _ = UnregisterHotKey(state.hwnd, id);
            }
            tracing::info!("unregistered toggle hotkey id={} mode='{}'", id, mode_id);
        }
        state.toggle_active = None;
        state.next_hotkey_id = 1;

        // Remove hold bindings and unhook
        state.hold_bindings.clear();
        state.hold_active = false;
        if let Some(hook) = state.hook.take() {
            unsafe {
                let _ = UnhookWindowsHookEx(hook);
            }
            tracing::info!("uninstalled low-level keyboard hook");
        }
    }

    fn handle_toggle_hotkey(state: &mut HotkeyThreadState, hotkey_id: i32) {
        let mode_id = state
            .toggle_modes
            .iter()
            .find(|(id, _)| *id == hotkey_id)
            .map(|(_, mode_id)| mode_id.clone());

        let Some(mode_id) = mode_id else { return };

        if state.toggle_active.as_deref() == Some(&mode_id) {
            // Currently recording with this mode -> stop
            state.toggle_active = None;
            let _ = state.event_tx.send(HotkeyEvent::StopRecording);
            tracing::info!("toggle stop for mode '{}'", mode_id);
        } else {
            // If recording with a different mode, stop it first
            if state.toggle_active.is_some() {
                let _ = state.event_tx.send(HotkeyEvent::StopRecording);
            }
            state.toggle_active = Some(mode_id.clone());
            let _ = state.event_tx.send(HotkeyEvent::StartRecording { mode_id: mode_id.clone() });
            tracing::info!("toggle start for mode '{}'", mode_id);
        }
    }

    /// Check if the current modifier key state matches the required modifiers.
    fn check_modifiers(required: u32) -> bool {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

        const VK_CONTROL: i32 = 0x11;
        const VK_SHIFT: i32 = 0x10;
        const VK_MENU: i32 = 0x12; // Alt

        const MOD_ALT: u32 = 0x0001;
        const MOD_CONTROL: u32 = 0x0002;
        const MOD_SHIFT: u32 = 0x0004;

        unsafe {
            let ctrl_down = GetAsyncKeyState(VK_CONTROL) as u16 & 0x8000 != 0;
            let shift_down = GetAsyncKeyState(VK_SHIFT) as u16 & 0x8000 != 0;
            let alt_down = GetAsyncKeyState(VK_MENU) as u16 & 0x8000 != 0;

            let ctrl_required = required & MOD_CONTROL != 0;
            let shift_required = required & MOD_SHIFT != 0;
            let alt_required = required & MOD_ALT != 0;

            ctrl_down == ctrl_required && shift_down == shift_required && alt_down == alt_required
        }
    }

    /// Low-level keyboard hook procedure for hold-mode hotkeys.
    unsafe extern "system" fn ll_keyboard_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code >= 0 {
            let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let vk = kb.vkCode;
            let is_down =
                wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
            let is_up = wparam.0 as u32 == WM_KEYUP || wparam.0 as u32 == WM_SYSKEYUP;

            THREAD_STATE.with(|cell| {
                let borrow = cell.borrow();
                if let Some(state_ptr) = *borrow {
                    let state = &mut *state_ptr;

                    // Find matching hold binding
                    let matching = state
                        .hold_bindings
                        .iter()
                        .find(|b| b.vk == vk && check_modifiers(b.modifiers));

                    if let Some(binding) = matching {
                        if is_down && !state.hold_active {
                            state.hold_active = true;
                            let _ = state.event_tx.send(HotkeyEvent::StartRecording {
                                mode_id: binding.mode_id.clone(),
                            });
                            tracing::debug!("hold start for mode '{}'", binding.mode_id);
                        }
                    }

                    // On key up of the active VK, stop recording
                    if is_up && state.hold_active {
                        let active_vk = state
                            .hold_bindings
                            .iter()
                            .any(|b| b.vk == vk);
                        if active_vk {
                            state.hold_active = false;
                            let _ = state.event_tx.send(HotkeyEvent::StopRecording);
                            tracing::debug!("hold stop (key up vk=0x{:X})", vk);
                        }
                    }
                }
            });
        }

        CallNextHookEx(None, code, wparam, lparam)
    }

    /// Minimal window proc - we handle most things in the GetMessage loop.
    unsafe extern "system" fn hotkey_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

// =============================================================================
// Non-Windows stub
// =============================================================================

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub struct HotkeyManager;

    impl HotkeyManager {
        pub fn new() -> Result<(Self, mpsc::UnboundedReceiver<HotkeyEvent>)> {
            let (_tx, rx) = mpsc::unbounded_channel();
            tracing::warn!("HotkeyManager: stub on non-Windows platform");
            Ok((Self, rx))
        }

        pub fn register(&self, _modes: &[ProcessingMode]) -> Result<()> {
            tracing::warn!("HotkeyManager::register() is a no-op on this platform");
            Ok(())
        }

        pub fn unregister_all(&self) -> Result<()> {
            tracing::warn!("HotkeyManager::unregister_all() is a no-op on this platform");
            Ok(())
        }

        pub fn shutdown(&self) {
            tracing::warn!("HotkeyManager::shutdown() is a no-op on this platform");
        }
    }
}

pub use platform::HotkeyManager;
