import AppKit

/// Invisible NSTextInputClient that attempts to receive IME input
/// without any visible window or focus steal.
///
/// Experiment: can NSTextInputContext.activate() route IME text
/// to a pure object (no view, no window) while another app has focus?
@MainActor
final class GhostTextClient: NSObject, NSTextInputClient {

    private(set) var buffer: String = ""
    private var inputContext: NSTextInputContext?
    private var markedString: String = ""
    private var markedSelectedRange: NSRange = .init(location: NSNotFound, length: 0)

    /// Called when text is committed by the IME (this is the key signal).
    var onTextInserted: ((String) -> Void)?

    /// Called when marked (composing) text changes.
    var onMarkedTextChanged: ((String) -> Void)?

    override init() {
        super.init()
        inputContext = NSTextInputContext(client: self)
        NSLog("[GhostText] Created, inputContext=%@", inputContext.map { "\($0)" } ?? "nil")
    }

    // MARK: - Activate / Deactivate

    func activate() {
        inputContext?.activate()
        NSLog("[GhostText] Activated")
    }

    func deactivate() {
        inputContext?.deactivate()
        NSLog("[GhostText] Deactivated")
    }

    func reset() {
        buffer = ""
        markedString = ""
    }

    // MARK: - NSTextInputClient (required)

    func insertText(_ string: Any, replacementRange: NSRange) {
        let text: String
        if let s = string as? String {
            text = s
        } else if let a = string as? NSAttributedString {
            text = a.string
        } else {
            return
        }
        buffer += text
        markedString = ""
        NSLog("[GhostText] ✅ insertText: '%@' (buffer now %d chars)", text.prefix(60) as NSString, buffer.count)
        onTextInserted?(text)
    }

    func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        if let s = string as? String {
            markedString = s
        } else if let a = string as? NSAttributedString {
            markedString = a.string
        }
        markedSelectedRange = selectedRange
        NSLog("[GhostText] setMarkedText: '%@'", markedString.prefix(60) as NSString)
        onMarkedTextChanged?(markedString)
    }

    func unmarkText() {
        markedString = ""
        markedSelectedRange = NSRange(location: NSNotFound, length: 0)
        NSLog("[GhostText] unmarkText")
    }

    func selectedRange() -> NSRange {
        NSRange(location: buffer.count, length: 0)
    }

    func markedRange() -> NSRange {
        if markedString.isEmpty {
            return NSRange(location: NSNotFound, length: 0)
        }
        return NSRange(location: buffer.count, length: markedString.count)
    }

    func hasMarkedText() -> Bool {
        !markedString.isEmpty
    }

    func attributedSubstring(forProposedRange range: NSRange, actualRange: NSRangePointer?) -> NSAttributedString? {
        guard range.location != NSNotFound,
              range.location >= 0,
              range.location + range.length <= buffer.count
        else { return nil }
        let start = buffer.index(buffer.startIndex, offsetBy: range.location)
        let end = buffer.index(start, offsetBy: range.length)
        let sub = String(buffer[start..<end])
        actualRange?.pointee = range
        return NSAttributedString(string: sub)
    }

    func validAttributesForMarkedText() -> [NSAttributedString.Key] {
        [.underlineStyle, .underlineColor]
    }

    func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
        // Return a rect near the mouse cursor so the IME's candidate window appears sensibly
        let mouse = NSEvent.mouseLocation
        actualRange?.pointee = range
        return NSRect(x: mouse.x, y: mouse.y - 30, width: 0, height: 20)
    }

    func characterIndex(for point: NSPoint) -> Int {
        buffer.count
    }

    func doCommand(by selector: Selector) {
        NSLog("[GhostText] doCommand: %@", NSStringFromSelector(selector))
    }
}
