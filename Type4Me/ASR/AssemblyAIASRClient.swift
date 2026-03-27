import Foundation
import os

enum AssemblyAIASRError: Error, LocalizedError {
    case unsupportedProvider
    case handshakeTimedOut
    case closedBeforeSessionBegan(code: Int, reason: String?)
    case unauthorized(reason: String?)
    case serverCancelled(reason: String?)
    case closed(code: Int, reason: String?)

    var errorDescription: String? {
        switch self {
        case .unsupportedProvider:
            return "AssemblyAIASRClient requires AssemblyAIASRConfig"
        case .handshakeTimedOut:
            return "AssemblyAI streaming session begin timed out"
        case .closedBeforeSessionBegan(let code, let reason):
            if let reason, !reason.isEmpty {
                return "AssemblyAI WebSocket closed before session begin (\(code)): \(reason)"
            }
            return "AssemblyAI WebSocket closed before session begin (\(code))"
        case .unauthorized(let reason):
            if let reason, !reason.isEmpty {
                return "AssemblyAI unauthorized connection: \(reason)"
            }
            return "AssemblyAI unauthorized connection"
        case .serverCancelled(let reason):
            if let reason, !reason.isEmpty {
                return "AssemblyAI session cancelled: \(reason)"
            }
            return "AssemblyAI session cancelled"
        case .closed(let code, let reason):
            if let reason, !reason.isEmpty {
                return "AssemblyAI session closed (\(code)): \(reason)"
            }
            return "AssemblyAI session closed (\(code))"
        }
    }
}

actor AssemblyAIASRClient: SpeechRecognizer {
    private let logger = Logger(
        subsystem: "com.type4me.asr",
        category: "AssemblyAIASRClient"
    )

    private var webSocketTask: URLSessionWebSocketTask?
    private var receiveTask: Task<Void, Never>?
    private var session: URLSession?
    private var sessionDelegate: AssemblyAIWebSocketDelegate?
    private var connectionGate: AssemblyAIConnectionGate?
    private var closeTracker: AssemblyAICloseTracker?

    private var eventContinuation: AsyncStream<RecognitionEvent>.Continuation?
    private var _events: AsyncStream<RecognitionEvent>?

    private var accumulator = AssemblyAITranscriptAccumulator()
    private var lastTranscript: RecognitionTranscript = .empty
    private var audioPacketCount = 0
    private var didRequestTerminate = false

    var events: AsyncStream<RecognitionEvent> {
        if let existing = _events {
            return existing
        }
        let (stream, continuation) = AsyncStream<RecognitionEvent>.makeStream()
        eventContinuation = continuation
        _events = stream
        return stream
    }

    func connect(config: any ASRProviderConfig, options: ASRRequestOptions = ASRRequestOptions()) async throws {
        guard let assemblyConfig = config as? AssemblyAIASRConfig else {
            throw AssemblyAIASRError.unsupportedProvider
        }

        let (stream, continuation) = AsyncStream<RecognitionEvent>.makeStream()
        eventContinuation = continuation
        _events = stream

        let url = try AssemblyAIProtocol.buildWebSocketURL(config: assemblyConfig, options: options)
        var request = URLRequest(url: url)
        request.setValue(assemblyConfig.apiKey, forHTTPHeaderField: "Authorization")

        let gate = AssemblyAIConnectionGate()
        let closeTracker = AssemblyAICloseTracker()
        let delegate = AssemblyAIWebSocketDelegate(connectionGate: gate, closeTracker: closeTracker)
        let session = URLSession(configuration: .default, delegate: delegate, delegateQueue: nil)
        let task = session.webSocketTask(with: request)
        task.resume()

        self.connectionGate = gate
        self.closeTracker = closeTracker
        sessionDelegate = delegate
        self.session = session
        webSocketTask = task
        accumulator = AssemblyAITranscriptAccumulator()
        lastTranscript = .empty
        audioPacketCount = 0
        didRequestTerminate = false

        startReceiveLoop()

        try await gate.waitUntilReady(timeout: .seconds(5))
        logger.info("AssemblyAI WebSocket connected: \(url.absoluteString, privacy: .private(mask: .hash))")
    }

    func sendAudio(_ data: Data) async throws {
        guard let task = webSocketTask else { return }
        audioPacketCount += 1
        try await task.send(.data(data))
    }

    func endAudio() async throws {
        guard let task = webSocketTask else { return }
        didRequestTerminate = true
        try await task.send(.string(AssemblyAIProtocol.terminateMessage()))
    }

    func disconnect() {
        receiveTask?.cancel()
        receiveTask = nil
        webSocketTask?.cancel(with: .normalClosure, reason: nil)
        webSocketTask = nil
        session?.invalidateAndCancel()
        session = nil
        sessionDelegate = nil
        connectionGate = nil
        closeTracker = nil
        eventContinuation?.finish()
        eventContinuation = nil
        _events = nil
        accumulator = AssemblyAITranscriptAccumulator()
        lastTranscript = .empty
        audioPacketCount = 0
        didRequestTerminate = false
        logger.info("AssemblyAI disconnected")
    }

    private func startReceiveLoop() {
        receiveTask = Task { [weak self] in
            guard let self else { return }
            while !Task.isCancelled {
                do {
                    guard let task = await self.webSocketTask else { break }
                    let message = try await task.receive()
                    await self.handleMessage(message)
                } catch {
                    if Task.isCancelled {
                        break
                    }

                    let gate = await self.connectionGate
                    let hasBegun = await gate?.hasBegun ?? false
                    let closeError = await self.closeTracker?.consumeCloseError()
                    let didRequestTerminate = await self.didRequestTerminate
                    let audioPacketCount = await self.audioPacketCount

                    if let gate, !hasBegun {
                        await gate.markFailure(closeError ?? error)
                    } else if let closeError, !didRequestTerminate {
                        await self.emitEvent(.error(closeError))
                        await self.emitEvent(.completed)
                    } else if didRequestTerminate || audioPacketCount > 0 {
                        await self.emitEvent(.completed)
                    } else {
                        await self.emitEvent(.error(error))
                        await self.emitEvent(.completed)
                    }
                    break
                }
            }

            let continuation = await self.eventContinuation
            continuation?.finish()
        }
    }

    private func handleMessage(_ message: URLSessionWebSocketTask.Message) async {
        do {
            let data: Data
            switch message {
            case .data(let payload):
                data = payload
            case .string(let text):
                data = Data(text.utf8)
            @unknown default:
                return
            }

            guard let event = try AssemblyAIProtocol.parseServerEvent(from: data) else {
                return
            }

            switch event {
            case .begin:
                if let gate = connectionGate {
                    await gate.markReady()
                }

            case .turn(let update):
                applyTurnUpdate(update)

            case .termination:
                emitEvent(.completed)

            case .speechStarted:
                break
            }
        } catch {
            if let gate = connectionGate {
                await gate.markFailure(error)
            }
            emitEvent(.error(error))
        }
    }

    private func applyTurnUpdate(_ update: AssemblyAITurnUpdate) {
        accumulator.apply(update)
        let transcript = accumulator.transcript
        guard transcript != lastTranscript else { return }
        lastTranscript = transcript
        emitEvent(.transcript(transcript))
    }
}

struct AssemblyAITranscriptAccumulator: Sendable {

    private struct TurnState: Sendable {
        let order: Int
        var finalizedText: String
        var displayText: String
        var isCompleted: Bool
        var isFormatted: Bool
    }

    private var turns: [Int: TurnState] = [:]

    mutating func apply(_ update: AssemblyAITurnUpdate) {
        let existing = turns[update.turnOrder]

        if let existing, existing.isCompleted, !update.isFinal {
            return
        }

        if update.isFinal {
            if let existing, existing.isFormatted, !update.isFormatted {
                return
            }
            turns[update.turnOrder] = TurnState(
                order: update.turnOrder,
                finalizedText: update.displayText,
                displayText: update.displayText,
                isCompleted: true,
                isFormatted: update.isFormatted
            )
        } else {
            turns[update.turnOrder] = TurnState(
                order: update.turnOrder,
                finalizedText: update.finalizedText,
                displayText: update.displayText,
                isCompleted: false,
                isFormatted: update.isFormatted
            )
        }
    }

    var transcript: RecognitionTranscript {
        let orderedTurns = turns.values.sorted { $0.order < $1.order }

        var confirmedSegments: [String] = []
        var existingText = ""
        var activeTurn: TurnState?

        for turn in orderedTurns {
            if turn.isCompleted {
                let normalized = normalize(segment: turn.displayText, after: existingText)
                confirmedSegments.append(normalized)
                existingText += normalized
            } else {
                activeTurn = turn
            }
        }

        var partialText = ""
        if let activeTurn {
            if !activeTurn.finalizedText.isEmpty {
                let normalizedFinalized = normalize(segment: activeTurn.finalizedText, after: existingText)
                confirmedSegments.append(normalizedFinalized)
                existingText += normalizedFinalized
            }

            let rawPartial: String
            if !activeTurn.finalizedText.isEmpty,
               activeTurn.displayText.hasPrefix(activeTurn.finalizedText) {
                rawPartial = String(activeTurn.displayText.dropFirst(activeTurn.finalizedText.count))
            } else if activeTurn.displayText == activeTurn.finalizedText {
                rawPartial = ""
            } else {
                rawPartial = activeTurn.displayText
            }

            partialText = normalize(segment: rawPartial, after: existingText)
        }

        let authoritativeText = (confirmedSegments + (partialText.isEmpty ? [] : [partialText])).joined()
        return RecognitionTranscript(
            confirmedSegments: confirmedSegments,
            partialText: partialText,
            authoritativeText: authoritativeText,
            isFinal: activeTurn == nil
        )
    }

    private func normalize(segment: String, after existingText: String) -> String {
        guard !segment.isEmpty else { return "" }
        guard let last = existingText.last else { return segment }
        guard let first = segment.first else { return segment }

        if last.isWhitespace || first.isWhitespace {
            return segment
        }

        if first.isClosingPunctuation || last.isOpeningPunctuation {
            return segment
        }

        if last.isCJKUnifiedIdeograph || first.isCJKUnifiedIdeograph {
            return segment
        }

        return " " + segment
    }
}

private actor AssemblyAICloseTracker {

    private var closeError: Error?

    func storeCloseError(_ error: Error) {
        closeError = error
    }

    func consumeCloseError() -> Error? {
        let error = closeError
        closeError = nil
        return error
    }
}

private extension AssemblyAIASRClient {
    func emitEvent(_ event: RecognitionEvent) {
        eventContinuation?.yield(event)
    }
}

private final class AssemblyAIWebSocketDelegate: NSObject, URLSessionWebSocketDelegate, URLSessionTaskDelegate {

    private let connectionGate: AssemblyAIConnectionGate
    private let closeTracker: AssemblyAICloseTracker

    init(connectionGate: AssemblyAIConnectionGate, closeTracker: AssemblyAICloseTracker) {
        self.connectionGate = connectionGate
        self.closeTracker = closeTracker
    }

    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didCompleteWithError error: Error?
    ) {
        guard let error else { return }
        Task {
            await connectionGate.markFailure(error)
        }
    }

    func urlSession(
        _ session: URLSession,
        webSocketTask: URLSessionWebSocketTask,
        didCloseWith closeCode: URLSessionWebSocketTask.CloseCode,
        reason: Data?
    ) {
        let reasonText = reason.flatMap { String(data: $0, encoding: .utf8) }
        let mappedError = AssemblyAIProtocol.makeCloseError(
            code: Int(closeCode.rawValue),
            reason: reasonText
        )
        Task {
            await closeTracker.storeCloseError(mappedError)
            guard await !connectionGate.hasBegun else { return }
            if case AssemblyAIASRError.unauthorized = mappedError {
                await connectionGate.markFailure(mappedError)
            } else if case AssemblyAIASRError.serverCancelled = mappedError {
                await connectionGate.markFailure(mappedError)
            } else {
                await connectionGate.markFailure(
                    AssemblyAIASRError.closedBeforeSessionBegan(
                        code: Int(closeCode.rawValue),
                        reason: reasonText
                    )
                )
            }
        }
    }
}

private extension Character {
    var isClosingPunctuation: Bool {
        ",.!?;:)]}\"'".contains(self)
    }

    var isOpeningPunctuation: Bool {
        "([{/\"'".contains(self)
    }

    var isCJKUnifiedIdeograph: Bool {
        unicodeScalars.contains { scalar in
            switch scalar.value {
            case 0x3400...0x4DBF, 0x4E00...0x9FFF, 0xF900...0xFAFF:
                return true
            default:
                return false
            }
        }
    }
}
