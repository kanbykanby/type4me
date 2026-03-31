import Foundation
import os

/// Manages the local ASR Python server process.
/// On Apple Silicon: starts Qwen3-ASR server (MLX/Metal).
/// On Intel: starts SenseVoice server (ONNX/CPU).
actor SenseVoiceServerManager {
    static let shared = SenseVoiceServerManager()

    /// Synchronous kill of all server processes. Safe to call from applicationWillTerminate.
    /// Reads PIDs from disk file, only kills processes we spawned.
    nonisolated static func killAllServerProcesses() {
        if let content = try? String(contentsOf: pidFileURL, encoding: .utf8) {
            for line in content.split(separator: "\n") {
                if let pid = Int32(line.trimmingCharacters(in: .whitespaces)), pid > 0 {
                    kill(pid, SIGTERM)
                }
            }
        }
        clearPidFile()
        currentPort = nil
        currentQwen3Port = nil
    }

    /// Write effective hotwords (builtin + user) to hotwords.txt for Python servers.
    /// Called from non-actor context (HotwordStorage.save, etc).
    nonisolated static func syncHotwordsFile() {
        let words = HotwordStorage.loadEffective()
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let dir = appSupport.appendingPathComponent("Type4Me")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let path = dir.appendingPathComponent("hotwords.txt")
        let content = words.joined(separator: "\n")
        try? content.write(to: path, atomically: true, encoding: .utf8)
        DebugFileLogger.log("Synced \(words.count) hotwords to hotwords.txt")
    }

    /// Sync hotwords and restart running servers to pick up changes.
    nonisolated static func syncHotwordsAndRestart() {
        syncHotwordsFile()
        Task {
            let mgr = shared
            let svWasRunning = await mgr.isRunning
            let q3WasRunning = await mgr.qwen3Port != nil
            if svWasRunning || q3WasRunning {
                await mgr.stop()
                try? await mgr.start()
                DebugFileLogger.log("Servers restarted for hotword update")
            }
        }
    }

    /// Whether this Mac has Apple Silicon (ARM64).
    private static let isAppleSilicon: Bool = {
        #if arch(arm64)
        return true
        #else
        return false
        #endif
    }()

    /// Port of the running SenseVoice server (primary, streaming).
    /// Set by actor-isolated `start()`, read by sync callers like KeychainService.
    nonisolated(unsafe) private(set) static var currentPort: Int?

    /// Port of the running Qwen3-ASR server (secondary, speculative final).
    /// Only set on Apple Silicon where both servers run.
    nonisolated(unsafe) private(set) static var currentQwen3Port: Int?

    private let logger = Logger(subsystem: "com.type4me.sensevoice", category: "ServerManager")

    private var process: Process?
    private(set) var port: Int?
    private var stdoutPipe: Pipe?

    private var qwen3Process: Process?
    private(set) var qwen3Port: Int?
    private var qwen3StdoutPipe: Pipe?

    var isRunning: Bool { process?.isRunning ?? false }

    var serverWSURL: URL? {
        guard let port else { return nil }
        return URL(string: "ws://127.0.0.1:\(port)/ws")
    }

    var healthURL: URL? {
        guard let port else { return nil }
        return URL(string: "http://127.0.0.1:\(port)/health")
    }

    var qwen3WSURL: URL? {
        guard let qwen3Port else { return nil }
        return URL(string: "ws://127.0.0.1:\(qwen3Port)/ws")
    }

    /// Called once at app launch. Kills orphans, then starts enabled servers.
    func start() async throws {
        killOrphanedServers()
        Self.syncHotwordsFile()

        let svEnabled = UserDefaults.standard.object(forKey: "tf_sensevoiceEnabled") as? Bool ?? true
        let qwen3Enabled = UserDefaults.standard.object(forKey: "tf_qwen3FinalEnabled") as? Bool ?? true

        DebugFileLogger.log("start(): sv=\(svEnabled) q3=\(qwen3Enabled)")

        // Launch enabled servers in parallel
        var qwen3Task: Task<Void, Error>?
        if Self.isAppleSilicon && qwen3Enabled && qwen3Process == nil {
            qwen3Task = Task { try await self.launchQwen3Server() }
        }

        if svEnabled && process == nil {
            try await launchSenseVoiceServer()
        }

        if let qwen3Task {
            do {
                try await qwen3Task.value
            } catch {
                logger.warning("Qwen3-ASR failed to start: \(error)")
                DebugFileLogger.log("Qwen3-ASR launch failed: \(error)")
            }
        }

        DebugFileLogger.log("start() done: svPort=\(Self.currentPort ?? -1) q3Port=\(Self.currentQwen3Port ?? -1)")
    }

    /// Launch the SenseVoice server as the primary streaming server.
    private func launchSenseVoiceServer() async throws {
        let proc = Process()
        var args: [String] = []

        try configureSenseVoiceServer(proc: proc, args: &args)

        // On Intel (no Qwen3), LLM runs on SenseVoice server
        if !Self.isAppleSilicon, let llmPath = LocalQwenLLMConfig.modelPath {
            args += ["--llm-model", llmPath]
            logger.info("LLM model configured on SenseVoice server: \(llmPath)")
        }

        proc.arguments = args

        let pipe = Pipe()
        proc.standardOutput = pipe
        let errPipe = Pipe()
        proc.standardError = errPipe
        errPipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty, let msg = String(data: data, encoding: .utf8) else { return }
            for line in msg.split(separator: "\n") where !line.isEmpty {
                DebugFileLogger.log("sensevoice-server: \(line)")
            }
        }
        self.stdoutPipe = pipe

        logger.info("Starting SenseVoice server: \(proc.executableURL?.path ?? "?")")

        do {
            try proc.run()
        } catch {
            logger.error("Failed to start SenseVoice server: \(error)")
            throw ServerError.launchFailed(error)
        }
        self.process = proc

        // SenseVoice model loading via PyTorch/FunASR is slow (~2 min), needs generous timeout
        let portResult = await readPortFromStdout(pipe: pipe, timeout: 180)
        guard let discoveredPort = portResult else {
            proc.terminate()
            self.process = nil
            throw ServerError.portDiscoveryFailed
        }
        self.port = discoveredPort
        Self.currentPort = discoveredPort
        logger.info("SenseVoice server started on port \(discoveredPort)")

        let healthy = await waitForHealth(timeout: 30)
        if !healthy {
            logger.warning("SenseVoice server started but health check not responding yet")
        }
        savePidsToFile()
    }

    /// Launch the Qwen3-ASR server as secondary (speculative final + LLM).
    private func launchQwen3Server() async throws {
        let proc = Process()
        var args: [String] = []

        try configureQwen3Server(proc: proc, args: &args)

        // LLM runs on Qwen3 server (shares _inference_lock for Metal GPU)
        if let llmPath = LocalQwenLLMConfig.modelPath {
            args += ["--llm-model", llmPath]
            logger.info("LLM model configured on Qwen3 server: \(llmPath)")
        }

        proc.arguments = args

        let pipe = Pipe()
        proc.standardOutput = pipe
        let errPipe = Pipe()
        proc.standardError = errPipe
        errPipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty, let msg = String(data: data, encoding: .utf8) else { return }
            for line in msg.split(separator: "\n") where !line.isEmpty {
                DebugFileLogger.log("qwen3-asr-server: \(line)")
            }
        }
        self.qwen3StdoutPipe = pipe

        logger.info("Starting Qwen3-ASR server: \(proc.executableURL?.path ?? "?")")

        do {
            try proc.run()
        } catch {
            logger.error("Failed to start Qwen3-ASR server: \(error)")
            throw ServerError.launchFailed(error)
        }
        self.qwen3Process = proc

        let portResult = await readPortFromStdout(pipe: pipe, timeout: 120)
        guard let discoveredPort = portResult else {
            proc.terminate()
            self.qwen3Process = nil
            throw ServerError.portDiscoveryFailed
        }
        self.qwen3Port = discoveredPort
        Self.currentQwen3Port = discoveredPort
        logger.info("Qwen3-ASR server started on port \(discoveredPort)")

        // Health check for Qwen3
        let qwen3HealthURL = URL(string: "http://127.0.0.1:\(discoveredPort)/health")!
        var healthy = false
        for _ in 0..<30 {
            do {
                let (_, response) = try await URLSession.shared.data(from: qwen3HealthURL)
                if (response as? HTTPURLResponse)?.statusCode == 200 { healthy = true; break }
            } catch {}
            try? await Task.sleep(for: .seconds(1))
        }
        if !healthy {
            logger.warning("Qwen3-ASR server started but health check not responding yet")
        }
        savePidsToFile()
    }

    /// Start the Qwen3-ASR server independently (e.g. when user enables verification).
    /// Start the SenseVoice server independently.
    func startSenseVoice() async throws {
        guard process == nil else { return }
        try await launchSenseVoiceServer()
    }

    /// Stop the SenseVoice server independently.
    func stopSenseVoice() {
        if let proc = process, proc.isRunning {
            proc.terminate()
        }
        process = nil
        port = nil
        Self.currentPort = nil
        stdoutPipe = nil
        logger.info("SenseVoice server stopped")
        DebugFileLogger.log("SenseVoice server stopped (user toggle)")
        savePidsToFile()
    }

    /// Start the Qwen3-ASR server independently.
    func startQwen3() async throws {
        guard qwen3Process == nil else { return }
        try await launchQwen3Server()
    }

    /// Stop the Qwen3-ASR server independently (e.g. when user disables verification).
    func stopQwen3() {
        if let proc = qwen3Process, proc.isRunning {
            proc.terminate()
        }
        qwen3Process = nil
        qwen3Port = nil
        Self.currentQwen3Port = nil
        qwen3StdoutPipe = nil
        logger.info("Qwen3-ASR server stopped")
        DebugFileLogger.log("Qwen3-ASR server stopped (user toggle)")
        savePidsToFile()
    }

    /// Stop all server processes.
    func stop() {
        if let proc = process, proc.isRunning {
            proc.terminate()
        }
        process = nil
        port = nil
        Self.currentPort = nil
        stdoutPipe = nil

        if let proc = qwen3Process, proc.isRunning {
            proc.terminate()
        }
        qwen3Process = nil
        qwen3Port = nil
        Self.currentQwen3Port = nil
        qwen3StdoutPipe = nil

        logger.info("All ASR servers stopped")
        savePidsToFile()  // Update (clear) PID file
    }

    // MARK: - PID File Management

    private static var pidFileURL: URL {
        let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return dir.appendingPathComponent("Type4Me/server-pids.txt")
    }

    /// Save current managed PIDs to disk so we can clean up after a crash.
    private func savePidsToFile() {
        var pids: [String] = []
        if let p = process, p.isRunning { pids.append(String(p.processIdentifier)) }
        if let p = qwen3Process, p.isRunning { pids.append(String(p.processIdentifier)) }
        try? pids.joined(separator: "\n").write(to: Self.pidFileURL, atomically: true, encoding: .utf8)
    }

    private static func clearPidFile() {
        try? FileManager.default.removeItem(at: pidFileURL)
    }

    /// Kill orphaned server processes from previous app runs using saved PID file.
    /// Only kills PIDs we previously spawned, never touches other users' processes.
    private func killOrphanedServers() {
        guard let content = try? String(contentsOf: Self.pidFileURL, encoding: .utf8) else { return }
        for line in content.split(separator: "\n") {
            guard let pid = Int32(line.trimmingCharacters(in: .whitespaces)), pid > 0 else { continue }
            // Verify process is still alive before killing
            if kill(pid, 0) == 0 {
                kill(pid, SIGTERM)
                DebugFileLogger.log("Killed orphaned server PID \(pid)")
            }
        }
        Self.clearPidFile()
    }

    /// Check if the server is healthy.
    nonisolated func isHealthy() async -> Bool {
        guard let url = await healthURL else { return false }
        do {
            let (_, response) = try await URLSession.shared.data(from: url)
            return (response as? HTTPURLResponse)?.statusCode == 200
        } catch {
            return false
        }
    }

    // MARK: - Private

    // MARK: - Qwen3-ASR (Apple Silicon)

    private func configureQwen3Server(proc: Process, args: inout [String]) throws {
        let serverScript: String
        let executable: String

        // Dev mode: qwen3-asr-server/.venv/bin/python + server.py
        // Production: bundled binary at Contents/MacOS/qwen3-asr-server
        let devDir = findDevServerDir(name: "qwen3-asr-server")
        if let dir = devDir {
            executable = (dir as NSString).appendingPathComponent(".venv/bin/python")
            serverScript = (dir as NSString).appendingPathComponent("server.py")
            guard FileManager.default.fileExists(atPath: executable) else {
                throw ServerError.venvNotFound
            }
        } else {
            let bundledBinary = Bundle.main.executableURL?
                .deletingLastPathComponent()
                .appendingPathComponent("qwen3-asr-server")
                .path
            guard let bin = bundledBinary, FileManager.default.fileExists(atPath: bin) else {
                throw ServerError.serverNotFound
            }
            executable = bin
            serverScript = ""
        }

        // Model path: bundled or ModelScope cache
        guard let modelPath = resolveQwen3ModelPath() else {
            throw ServerError.modelNotFound
        }
        logger.info("Qwen3-ASR model: \(modelPath)")

        // Hotwords file (same as SenseVoice)
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let hotwordsPath = appSupport
            .appendingPathComponent("Type4Me")
            .appendingPathComponent("hotwords.txt")
        let hotwordsFile = FileManager.default.fileExists(atPath: hotwordsPath.path) ? hotwordsPath.path : ""

        proc.executableURL = URL(fileURLWithPath: executable)
        if !serverScript.isEmpty {
            args.append(serverScript)
        }
        args += [
            "--model-path", modelPath,
            "--port", "0",
            "--hotwords-file", hotwordsFile,
        ]
        logger.info("Starting Qwen3-ASR server")
    }

    private func resolveQwen3ModelPath() -> String? {
        // 1. Bundled in app (production DMG)
        let bundled = Bundle.main.resourceURL?
            .appendingPathComponent("Models")
            .appendingPathComponent("Qwen3-ASR")
        if let b = bundled, FileManager.default.fileExists(atPath: b.path) {
            return b.path
        }
        // 2. App Support (user-downloaded)
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let userModel = appSupport
            .appendingPathComponent("Type4Me")
            .appendingPathComponent("Models/Qwen3-ASR")
        if FileManager.default.fileExists(atPath: userModel.path) {
            return userModel.path
        }
        // 3. ModelScope cache (dev fallback)
        let cache06 = NSHomeDirectory() + "/.cache/modelscope/hub/models/Qwen/Qwen3-ASR-0.6B"
        if FileManager.default.fileExists(atPath: cache06) { return cache06 }
        let cache17 = NSHomeDirectory() + "/.cache/modelscope/hub/models/Qwen/Qwen3-ASR-1.7B"
        if FileManager.default.fileExists(atPath: cache17) { return cache17 }
        return nil
    }

    // MARK: - SenseVoice (Intel fallback)

    private func configureSenseVoiceServer(proc: Process, args: inout [String]) throws {
        let serverScript: String
        let executable: String

        let devDir = findDevServerDir(name: "sensevoice-server")
        if let dir = devDir {
            executable = (dir as NSString).appendingPathComponent(".venv/bin/python")
            serverScript = (dir as NSString).appendingPathComponent("server.py")
            guard FileManager.default.fileExists(atPath: executable) else {
                throw ServerError.venvNotFound
            }
        } else {
            let bundledBinary = Bundle.main.executableURL?
                .deletingLastPathComponent()
                .appendingPathComponent("sensevoice-server")
                .path
            guard let bin = bundledBinary, FileManager.default.fileExists(atPath: bin) else {
                throw ServerError.serverNotFound
            }
            executable = bin
            serverScript = ""
        }

        let bundledModel = Bundle.main.resourceURL?
            .appendingPathComponent("Models")
            .appendingPathComponent("SenseVoiceSmall")
        let modelDir: String
        if let bundled = bundledModel, FileManager.default.fileExists(atPath: bundled.path) {
            modelDir = bundled.path
        } else {
            // Check ModelScope cache: if model.pt exists, use the local path directly
            // to avoid ModelScope re-downloading due to stale metadata (.mdl corruption).
            let cacheDir = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(".cache/modelscope/hub/models/iic/SenseVoiceSmall")
            let cachedModel = cacheDir.appendingPathComponent("model.pt")
            if FileManager.default.fileExists(atPath: cachedModel.path) {
                modelDir = cacheDir.path
                logger.info("Using ModelScope cached model at \(cacheDir.path)")
            } else {
                modelDir = "iic/SenseVoiceSmall"
                logger.info("No cached model, will download from ModelScope")
            }
        }

        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let hotwordsPath = appSupport
            .appendingPathComponent("Type4Me")
            .appendingPathComponent("hotwords.txt")
        let hotwordsFile = FileManager.default.fileExists(atPath: hotwordsPath.path) ? hotwordsPath.path : ""

        proc.executableURL = URL(fileURLWithPath: executable)
        if !serverScript.isEmpty {
            args.append(serverScript)
        }
        args += [
            "--model-dir", modelDir,
            "--port", "0",
            "--hotwords-file", hotwordsFile,
            "--beam-size", "3",
            "--context-score", "6.0",
            "--device", "auto",
            "--language", "auto",
            "--textnorm",
            "--padding", "8",
            "--chunk-size", "10",
        ]
        logger.info("Starting SenseVoice server")
    }

    // MARK: - Dev server discovery

    private func findDevServerDir(name: String) -> String? {
        // Walk up from binary location to find server directory
        var dir = Bundle.main.bundlePath
        for _ in 0..<5 {
            dir = (dir as NSString).deletingLastPathComponent
            let candidate = (dir as NSString).appendingPathComponent(name)
            if FileManager.default.fileExists(atPath: (candidate as NSString).appendingPathComponent("server.py")) {
                return candidate
            }
        }
        let home = NSHomeDirectory()
        let fallback = (home as NSString).appendingPathComponent("projects/type4me/\(name)")
        if FileManager.default.fileExists(atPath: (fallback as NSString).appendingPathComponent("server.py")) {
            return fallback
        }
        return nil
    }

    private func readPortFromStdout(pipe: Pipe, timeout: Int) async -> Int? {
        return await withCheckedContinuation { continuation in
            let handle = pipe.fileHandleForReading
            let lock = NSLock()
            var resolved = false

            // Read in background
            DispatchQueue.global().async {
                while true {
                    let data = handle.availableData
                    guard !data.isEmpty else { break }
                    if let output = String(data: data, encoding: .utf8) {
                        for line in output.split(separator: "\n") {
                            if line.hasPrefix("PORT:"),
                               let portNum = Int(line.dropFirst(5)) {
                                lock.lock()
                                guard !resolved else { lock.unlock(); return }
                                resolved = true
                                lock.unlock()
                                continuation.resume(returning: portNum)
                                return
                            }
                        }
                    }
                }
                lock.lock()
                guard !resolved else { lock.unlock(); return }
                resolved = true
                lock.unlock()
                continuation.resume(returning: nil)
            }

            // Timeout
            DispatchQueue.global().asyncAfter(deadline: .now() + .seconds(timeout)) {
                lock.lock()
                guard !resolved else { lock.unlock(); return }
                resolved = true
                lock.unlock()
                continuation.resume(returning: nil)
            }
        }
    }

    private func waitForHealth(timeout: Int) async -> Bool {
        for _ in 0..<timeout {
            if await isHealthy() { return true }
            try? await Task.sleep(for: .seconds(1))
        }
        return false
    }

    // MARK: - Errors

    enum ServerError: Error, LocalizedError {
        case serverNotFound
        case venvNotFound
        case modelNotFound
        case launchFailed(Error)
        case portDiscoveryFailed

        var errorDescription: String? {
            switch self {
            case .serverNotFound:
                return L("SenseVoice 服务未找到", "SenseVoice server not found")
            case .venvNotFound:
                return L("Python 环境未配置", "Python environment not configured")
            case .modelNotFound:
                return L("本地 ASR 模型未找到，请先下载", "Local ASR model not found, please download first")
            case .launchFailed(let e):
                return L("服务启动失败: \(e.localizedDescription)", "Server launch failed: \(e.localizedDescription)")
            case .portDiscoveryFailed:
                return L("服务端口发现失败", "Server port discovery failed")
            }
        }
    }
}
