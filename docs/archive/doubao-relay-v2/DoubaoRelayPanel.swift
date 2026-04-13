import AppKit

/// Invisible activating panel that hosts a hidden NSTextView to receive DoubaoIme input.
/// The DISPLAY is handled by the existing FloatingBar (driven via AppState).
/// This panel only exists to be key window so the IME routes text here.
@MainActor
final class DoubaoRelayIMEPanel: NSPanel {

    let textView: NSTextView

    /// Called on main actor whenever IME text changes.
    var onTextChange: ((String) -> Void)?

    init() {
        let tv = NSTextView(frame: NSRect(x: 0, y: 0, width: 100, height: 20))
        tv.isEditable = true
        tv.isSelectable = true
        tv.isRichText = false
        tv.font = .systemFont(ofSize: 14)
        tv.textColor = .clear
        tv.backgroundColor = .clear
        tv.drawsBackground = false
        tv.insertionPointColor = .clear
        tv.isAutomaticQuoteSubstitutionEnabled = false
        tv.isAutomaticDashSubstitutionEnabled = false
        tv.isAutomaticTextReplacementEnabled = false
        tv.isAutomaticSpellingCorrectionEnabled = false
        tv.isVerticallyResizable = false
        tv.isHorizontallyResizable = true
        tv.textContainer?.widthTracksTextView = false
        tv.textContainer?.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: 20)
        tv.textContainerInset = .zero
        self.textView = tv

        // Small transparent panel, on-screen but invisible to user.
        // Must be on-screen and reasonably sized for IME to accept it.
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 100, height: 20),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )

        isFloatingPanel = true
        level = .floating
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        animationBehavior = .none
        appearance = NSAppearance(named: .darkAqua)
        hidesOnDeactivate = false

        let root = NSView(frame: NSRect(x: 0, y: 0, width: 1, height: 1))
        root.addSubview(tv)
        contentView = root

        tv.textStorage?.delegate = self
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    func activate() {
        textView.string = ""
        let screen = NSScreen.main ?? NSScreen.screens.first
        let vis = screen?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        setFrameOrigin(NSPoint(x: vis.maxX - 110, y: vis.minY))
        alphaValue = 0.01

        orderFrontRegardless()
        NSApp.activate(ignoringOtherApps: true)
        makeKeyAndOrderFront(nil)
        makeFirstResponder(textView)
    }

    func deactivate() {
        orderOut(nil)
    }

    var currentText: String { textView.string }
}

extension DoubaoRelayIMEPanel: NSTextStorageDelegate {
    nonisolated func textStorage(
        _ textStorage: NSTextStorage,
        didProcessEditing editedMask: NSTextStorageEditActions,
        range editedRange: NSRange,
        changeInLength delta: Int
    ) {
        guard editedMask.contains(.editedCharacters) else { return }
        let text = textStorage.string
        Task { @MainActor in
            self.onTextChange?(text)
        }
    }
}
