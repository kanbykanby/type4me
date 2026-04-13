import AppKit
import ApplicationServices
import Foundation

extension Notification.Name {
    static let doubaoIntegrationDidChange = Notification.Name("Type4Me.doubaoIntegrationDidChange")
}

/// Coordinates DoubaoIme ASR observation with post-processing.
/// v2 "relay" mode: ASR text flows into a Type4Me-owned panel,
/// then gets snippet-replaced / LLM-processed / injected into the original app.
@MainActor
final class DoubaoIntegrationController: DoubaoASRObserver.Delegate {

    private let observer = DoubaoASRObserver()
    private let postProcessor = PostProcessorSession()

    /// Set by AppDelegate so we can suppress the tap during simulated key events.
    weak var hotkeyManager: HotkeyManager?

    // MARK: - Relay (v2): invisible IME panel + FloatingBar display via AppState

    private let imePanel = DoubaoRelayIMEPanel()

    /// Set by AppDelegate to drive the existing FloatingBar.
    weak var appState: AppState?

    /// The app that had focus before we started the relay session.
    private var savedFrontApp: NSRunningApplication?

    /// The mode armed for this relay session.
    private var relayMode: ProcessingMode?

    /// True while a relay session is in progress.
    private(set) var isRelayActive: Bool = false

    /// Generation counter to prevent stale timeout tasks from killing new sessions.
    private var relayGeneration: Int = 0

    // MARK: - LLM Mode Arming (v1 legacy, kept for non-relay usage)

    private(set) var armedMode: ProcessingMode?
    var onArmedModeChanged: ((ProcessingMode?) -> Void)?

    /// The key code for DoubaoIme's ASR trigger.
    /// Default: Left Option (58). Triggered via long-press, not double-tap.
    static let doubaoHotkeyKey = "tf_doubaoASRKeyCode"
    private var doubaoASRKeyCode: CGKeyCode {
        let stored = UserDefaults.standard.integer(forKey: Self.doubaoHotkeyKey)
        return stored > 0 ? CGKeyCode(stored) : 58  // Left Option
    }

    private(set) var preArmedCursorPos: Int?
    private(set) var preArmedElement: AXUIElement?

    func armLLMMode(_ mode: ProcessingMode) {
        armedMode = mode
        onArmedModeChanged?(mode)
        let snapshot = observer.readFocusedTextFieldState()
        preArmedCursorPos = snapshot.cursorPosition
        preArmedElement = snapshot.element
        observer.overrideStartElement = snapshot.element
        observer.overrideStartCursorPos = snapshot.cursorPosition
        DebugFileLogger.log("[DoubaoIntegration] Armed LLM mode: \(mode.name), preCursor=\(snapshot.cursorPosition.map(String.init) ?? "nil")")
    }

    func disarmLLMMode() {
        armedMode = nil
        onArmedModeChanged?(nil)
        DebugFileLogger.log("[DoubaoIntegration] Disarmed LLM mode")
    }

    // MARK: - Relay Session (v2)

    /// Start a relay session: show panel → trigger DoubaoIme ASR.
    /// Text flows into the relay panel instead of the target app.
    func startRelaySession(mode: ProcessingMode) {
        guard !isRelayActive else {
            DebugFileLogger.log("[DoubaoRelay] Already active, ignoring duplicate start")
            return
        }

        relayGeneration += 1
        let myGeneration = relayGeneration
        isRelayActive = true
        relayMode = mode
        savedFrontApp = NSWorkspace.shared.frontmostApplication

        DebugFileLogger.log("[DoubaoRelay] Starting relay session #\(myGeneration): mode=\(mode.name), savedApp=\(savedFrontApp?.localizedName ?? "nil")")

        // 1. Position FloatingBar over doubao indicator, show preparing state
        FloatingBarPanel.positionOverDoubao = true
        if let appState {
            appState.currentMode = mode
            appState.startRecording()   // barPhase → .preparing (centered circle + spinner)
        }

        // 2. Bridge IME text → FloatingBar display
        imePanel.onTextChange = { [weak self] text in
            self?.appState?.segments = [TranscriptionSegment(text: text, isConfirmed: true)]
        }

        // 3. Activate IME panel + trigger doubao ASR + transition to recording
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(150))
            guard self.isRelayActive, self.relayGeneration == myGeneration else { return }

            self.imePanel.activate()

            try? await Task.sleep(for: .milliseconds(100))
            guard self.isRelayActive, self.relayGeneration == myGeneration else { return }
            self.triggerDoubaoASR()

            // Ready: transition to recording
            try? await Task.sleep(for: .milliseconds(150))
            guard self.isRelayActive else { return }
            self.appState?.markRecordingReady()
            SoundFeedback.playStart()

            // Lower volume after sound
            let targetVolumePercent = UserDefaults.standard.integer(forKey: "tf_volumeReduction")
            if targetVolumePercent >= 0 {
                try? await Task.sleep(for: .milliseconds(500))
                guard self.isRelayActive, self.appState?.barPhase == .recording else { return }
                SystemVolumeManager.lower(to: Float(targetVolumePercent) / 100.0)
            }
        }
    }

    /// User pressed toggle-stop: release Option key, then complete relay.
    /// If observer detects ASR end it calls onRelayASRComplete.
    /// Fallback: if observer doesn't fire within 1s, complete directly.
    func finishRelaySession() {
        guard isRelayActive else { return }
        let myGeneration = relayGeneration
        DebugFileLogger.log("[DoubaoRelay] Finish requested, stopping ASR")
        stopDoubaoASR()
        SoundFeedback.playStop()
        SystemVolumeManager.restore()

        // Fallback: if observer doesn't trigger onRelayASRComplete within 1s
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(1))
            guard self.isRelayActive, self.relayGeneration == myGeneration else { return }
            DebugFileLogger.log("[DoubaoRelay] Observer fallback: completing relay directly")
            self.onRelayASRComplete()
        }
    }

    /// Cancel a relay session: stop ASR, hide panel, restore focus, don't inject.
    func cancelRelay() {
        guard isRelayActive else { return }
        DebugFileLogger.log("[DoubaoRelay] Cancelled")

        stopDoubaoASR()
        cleanUpRelay()
    }

    /// Called when DoubaoASRObserver detects ASR ended during a relay session.
    private func onRelayASRComplete() {
        guard isRelayActive else { return }

        let rawText = imePanel.currentText.trimmingCharacters(in: .whitespacesAndNewlines)
        let mode = relayMode

        DebugFileLogger.log("[DoubaoRelay] ASR complete: \(rawText.count) chars, mode=\(mode?.name ?? "none")")

        // Clean up relay state
        imePanel.deactivate()
        FloatingBarPanel.positionOverDoubao = false
        let targetApp = savedFrontApp

        isRelayActive = false
        relayMode = nil
        savedFrontApp = nil
        hotkeyManager?.resetActiveState()

        guard !rawText.isEmpty else {
            DebugFileLogger.log("[DoubaoRelay] Empty text, skipping")
            appState?.cancel()
            restoreFocus(to: targetApp)
            return
        }

        // Show processing state in FloatingBar
        appState?.stopRecording()

        // Process: snippet → restore focus → optional LLM → paste → history
        Task { @MainActor in
            let snippetResult = SnippetStorage.applyEffective(to: rawText)
            DebugFileLogger.log("[DoubaoRelay] Snippet: '\(rawText.prefix(40))' → '\(snippetResult.prefix(40))'")

            self.restoreFocus(to: targetApp)
            try? await Task.sleep(for: .milliseconds(300))

            let startTime = Date()
            var finalText = snippetResult

            if let mode, !mode.prompt.isEmpty {
                let placeholder = mode.processingLabel + "..."
                self.pasteText(placeholder)

                if let llmResult = await self.runLLM(text: snippetResult, mode: mode) {
                    finalText = llmResult
                }

                try? await Task.sleep(for: .milliseconds(50))
                self.simulateBackspace(count: placeholder.count)
                try? await Task.sleep(for: .milliseconds(80))
                self.pasteText(finalText)
            } else {
                self.pasteText(finalText)
            }

            self.appState?.finalize(text: finalText, outcome: .inserted)

            let duration = Date().timeIntervalSince(startTime)
            let record = HistoryRecord(
                id: UUID().uuidString,
                createdAt: startTime,
                durationSeconds: duration,
                rawText: rawText,
                processingMode: mode?.name,
                processedText: finalText != rawText ? finalText : nil,
                finalText: finalText,
                status: "completed",
                characterCount: finalText.count,
                asrProvider: "DoubaoRelay"
            )
            await self.postProcessor.historyStore.insert(record)
            DebugFileLogger.log("[DoubaoRelay] History recorded: \(finalText.count) chars")
        }
    }

    private func restoreFocus(to app: NSRunningApplication?) {
        guard let app else { return }
        app.activate()
        DebugFileLogger.log("[DoubaoRelay] Restored focus to \(app.localizedName ?? "?")")
    }

    private func cleanUpRelay() {
        imePanel.deactivate()
        FloatingBarPanel.positionOverDoubao = false
        SystemVolumeManager.restore()
        appState?.showCancelled()
        restoreFocus(to: savedFrontApp)
        isRelayActive = false
        relayMode = nil
        savedFrontApp = nil
        hotkeyManager?.resetActiveState()
    }

    // MARK: - LLM Helper

    private func runLLM(text: String, mode: ProcessingMode) async -> String? {
        guard let llmConfig = KeychainService.loadLLMConfig() else {
            DebugFileLogger.log("[DoubaoRelay] LLM skipped: no config")
            return nil
        }

        let client: any LLMClient = KeychainService.selectedLLMProvider == .claude
            ? ClaudeChatClient()
            : DoubaoChatClient(provider: KeychainService.selectedLLMProvider)

        let context = await PromptContext.capture()
        let prompt = context.expandContextVariables(mode.prompt)

        DebugFileLogger.log("[DoubaoRelay] LLM starting: model=\(llmConfig.model), \(text.count) chars")

        do {
            let result = try await withThrowingTaskGroup(of: String.self) { group in
                group.addTask { try await client.process(text: text, prompt: prompt, config: llmConfig) }
                group.addTask { try await Task.sleep(for: .seconds(15)); throw CancellationError() }
                let r = try await group.next()!
                group.cancelAll()
                return r
            }
            guard !result.isEmpty else { return nil }
            DebugFileLogger.log("[DoubaoRelay] LLM done: \(result.count) chars")
            return result
        } catch {
            DebugFileLogger.log("[DoubaoRelay] LLM failed: \(error.localizedDescription)")
            return nil
        }
    }

    // MARK: - Simulate Keys

    /// Double-tap Option key to start DoubaoIme ASR (toggle mode).
    func triggerDoubaoASR() {
        let keyCode = doubaoASRKeyCode
        DebugFileLogger.log("[DoubaoIntegration] Double-tap START: keyCode=\(keyCode)")

        Task.detached { [weak self] in
            self?.postModifierKeyEvent(keyCode: keyCode, isPress: true)
            usleep(30_000)
            self?.postModifierKeyEvent(keyCode: keyCode, isPress: false)
            usleep(80_000)
            self?.postModifierKeyEvent(keyCode: keyCode, isPress: true)
            usleep(30_000)
            self?.postModifierKeyEvent(keyCode: keyCode, isPress: false)
        }
    }

    /// Double-tap Option key to stop DoubaoIme ASR (toggle mode).
    func stopDoubaoASR() {
        let keyCode = doubaoASRKeyCode
        DebugFileLogger.log("[DoubaoIntegration] Double-tap STOP: keyCode=\(keyCode)")

        Task.detached { [weak self] in
            self?.postModifierKeyEvent(keyCode: keyCode, isPress: true)
            usleep(30_000)
            self?.postModifierKeyEvent(keyCode: keyCode, isPress: false)
            usleep(80_000)
            self?.postModifierKeyEvent(keyCode: keyCode, isPress: true)
            usleep(30_000)
            self?.postModifierKeyEvent(keyCode: keyCode, isPress: false)
        }
    }

    nonisolated private func postModifierKeyEvent(keyCode: CGKeyCode, isPress: Bool) {
        guard let event = CGEvent(source: nil) else { return }
        event.type = .flagsChanged
        event.setIntegerValueField(.keyboardEventKeycode, value: Int64(keyCode))
        event.flags = isPress ? flagsForModifierKey(keyCode) : []
        event.post(tap: .cghidEventTap)
    }

    nonisolated private func flagsForModifierKey(_ keyCode: CGKeyCode) -> CGEventFlags {
        switch keyCode {
        case 54, 55: return .maskCommand
        case 56, 60: return .maskShift
        case 58, 61: return .maskAlternate
        case 59, 62: return .maskControl
        default: return []
        }
    }

    /// Write text to clipboard and simulate Cmd+V. Simple, no save/restore.
    private func pasteText(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        usleep(50_000)
        let vKeyCode: CGKeyCode = 9
        if let keyDown = CGEvent(keyboardEventSource: nil, virtualKey: vKeyCode, keyDown: true),
           let keyUp = CGEvent(keyboardEventSource: nil, virtualKey: vKeyCode, keyDown: false) {
            keyDown.flags = .maskCommand
            keyUp.flags = .maskCommand
            keyDown.post(tap: .cghidEventTap)
            keyUp.post(tap: .cghidEventTap)
        }
        usleep(100_000)
    }

    private func simulateBackspace(count: Int) {
        let backspaceKeyCode: CGKeyCode = 51
        for _ in 0..<count {
            guard let keyDown = CGEvent(keyboardEventSource: nil, virtualKey: backspaceKeyCode, keyDown: true),
                  let keyUp = CGEvent(keyboardEventSource: nil, virtualKey: backspaceKeyCode, keyDown: false)
            else { continue }
            keyDown.post(tap: .cghidEventTap)
            keyUp.post(tap: .cghidEventTap)
            usleep(5_000)
        }
    }

    // MARK: - Lifecycle

    static let enabledKey = "tf_doubaoIntegrationEnabled"

    var isEnabled: Bool {
        UserDefaults.standard.bool(forKey: Self.enabledKey)
    }

    func startIfEnabled() {
        let enabled = isEnabled
        DebugFileLogger.log("DoubaoIntegration startIfEnabled: \(enabled)")
        guard enabled else { return }
        start()
    }

    func start() {
        observer.delegate = self
        observer.startObserving()
        registerForHookNotifications()
        NSLog("[DoubaoIntegration] Started")
    }

    func stop() {
        observer.stopObserving()
        disarmLLMMode()
        if isRelayActive { cleanUpRelay() }
        NSLog("[DoubaoIntegration] Stopped")
    }

    // MARK: - Hook Notification (v1 legacy, for non-relay snippet replacement)

    private var hookNotificationObserver: Any?

    private func registerForHookNotifications() {
        hookNotificationObserver = DistributedNotificationCenter.default().addObserver(
            forName: NSNotification.Name("Type4Me.DoubaoASRTextInserted"),
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let userInfo = notification.userInfo,
                  let rawText = userInfo["rawText"] as? String,
                  let processedText = userInfo["processedText"] as? String,
                  let charCount = userInfo["charCount"] as? Int,
                  processedText != rawText
            else { return }

            Task { @MainActor [weak self] in
                guard let self else { return }

                let elapsed = Date().timeIntervalSince(self.lastReplacementTime)
                if elapsed < Self.cooldownSeconds {
                    DebugFileLogger.log("[DoubaoIntegration] Cooldown skip (\(String(format: "%.1f", elapsed))s)")
                    return
                }

                DebugFileLogger.log("[DoubaoIntegration] Hook notification: \(charCount) chars → '\(processedText.prefix(30))'")
                try? await Task.sleep(for: .milliseconds(100))

                let bundleID = NSWorkspace.shared.frontmostApplication?.bundleIdentifier ?? ""
                let isTerminal = Self.terminalBundleIDs.contains(bundleID)

                if isTerminal {
                    await self.replaceViaBackspace(charCount: charCount, replacement: processedText)
                } else {
                    await self.replaceViaUndo(charCount: charCount, replacement: processedText)
                }
                self.lastReplacementTime = Date()
            }
        }
    }

    private var isReplacing = false
    private var lastReplacementTime: Date = .distantPast
    private static let cooldownSeconds: TimeInterval = 1.0

    private static let terminalBundleIDs: Set<String> = [
        "com.googlecode.iterm2",
        "com.apple.Terminal",
        "dev.warp.Warp-Stable",
        "com.github.wez.wezterm",
        "io.alacritty",
    ]

    private func replaceViaUndo(charCount: Int, replacement: String) async {
        while isReplacing { try? await Task.sleep(for: .milliseconds(50)) }
        isReplacing = true
        defer { isReplacing = false }

        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(replacement, forType: .string)
        try? await Task.sleep(for: .milliseconds(30))

        let zKeyCode: CGKeyCode = 6
        if let keyDown = CGEvent(keyboardEventSource: nil, virtualKey: zKeyCode, keyDown: true),
           let keyUp = CGEvent(keyboardEventSource: nil, virtualKey: zKeyCode, keyDown: false) {
            keyDown.flags = .maskCommand
            keyUp.flags = .maskCommand
            keyDown.post(tap: .cghidEventTap)
            keyUp.post(tap: .cghidEventTap)
        }
        try? await Task.sleep(for: .milliseconds(100))

        let vKeyCode: CGKeyCode = 9
        if let keyDown = CGEvent(keyboardEventSource: nil, virtualKey: vKeyCode, keyDown: true),
           let keyUp = CGEvent(keyboardEventSource: nil, virtualKey: vKeyCode, keyDown: false) {
            keyDown.flags = .maskCommand
            keyUp.flags = .maskCommand
            keyDown.post(tap: .cghidEventTap)
            keyUp.post(tap: .cghidEventTap)
        }
        try? await Task.sleep(for: .milliseconds(100))
        DebugFileLogger.log("[DoubaoIntegration] Replaced via undo: \(charCount) chars → '\(replacement.prefix(30))'")
    }

    private func replaceViaBackspace(charCount: Int, replacement: String) async {
        while isReplacing { try? await Task.sleep(for: .milliseconds(50)) }
        isReplacing = true
        defer { isReplacing = false }

        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(replacement, forType: .string)
        try? await Task.sleep(for: .milliseconds(30))

        let backspaceKeyCode: CGKeyCode = 51
        let batchSize = 10
        for i in 0..<charCount {
            guard let keyDown = CGEvent(keyboardEventSource: nil, virtualKey: backspaceKeyCode, keyDown: true),
                  let keyUp = CGEvent(keyboardEventSource: nil, virtualKey: backspaceKeyCode, keyDown: false)
            else { continue }
            keyDown.post(tap: .cghidEventTap)
            keyUp.post(tap: .cghidEventTap)
            usleep(5_000)
            if (i + 1) % batchSize == 0 { usleep(30_000) }
        }

        let waitMs = max(200, charCount * 3)
        try? await Task.sleep(for: .milliseconds(waitMs))

        let vKeyCode: CGKeyCode = 9
        if let keyDown = CGEvent(keyboardEventSource: nil, virtualKey: vKeyCode, keyDown: true),
           let keyUp = CGEvent(keyboardEventSource: nil, virtualKey: vKeyCode, keyDown: false) {
            keyDown.flags = .maskCommand
            keyUp.flags = .maskCommand
            keyDown.post(tap: .cghidEventTap)
            keyUp.post(tap: .cghidEventTap)
        }
        try? await Task.sleep(for: .milliseconds(100))
        DebugFileLogger.log("[DoubaoIntegration] Replaced via backspace: \(charCount) chars → '\(replacement.prefix(30))'")
    }

    // MARK: - ASR Observer Delegate

    private var pendingElement: AXUIElement?
    private var pendingStartPos: Int?

    nonisolated func doubaoASRDidStart(element: AXUIElement, cursorPosition: Int) {
        Task { @MainActor in
            self.pendingElement = element
            self.pendingStartPos = cursorPosition
        }
    }

    nonisolated func doubaoASRDidEnd(
        element: AXUIElement,
        startCursorPosition: Int,
        endCursorPosition: Int,
        asrText: String
    ) {
        Task { @MainActor in
            // v2 relay mode: read from IME panel
            if self.isRelayActive {
                self.onRelayASRComplete()
                return
            }

            // v1 legacy flow (non-relay)
            guard !asrText.isEmpty else { return }
            let mode = self.armedMode
            if mode != nil { self.disarmLLMMode() }

            let asr = PostProcessorSession.ASRResult(
                element: element,
                startPos: startCursorPosition,
                endPos: endCursorPosition,
                rawText: asrText
            )
            await self.postProcessor.process(asr: asr, armedMode: mode)
        }
    }
}
