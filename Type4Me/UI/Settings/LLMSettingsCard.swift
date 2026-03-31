import SwiftUI

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// MARK: - LLM Settings Card
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

struct LLMSettingsCard: View, SettingsCardHelpers {

    @State private var selectedLLMProvider: LLMProvider = .doubao
    @State private var llmCredentialValues: [String: String] = [:]
    @State private var savedLLMValues: [String: String] = [:]
    @State private var editedFields: Set<String> = []
    @State private var llmTestStatus: SettingsTestStatus = .idle
    @State private var isEditingLLM = true
    @State private var hasStoredLLM = false
    @State private var testTask: Task<Void, Never>?
    @State private var serverStarting = false
    @State private var serverRunning = false

    private var currentLLMFields: [CredentialField] {
        LLMProviderRegistry.configType(for: selectedLLMProvider)?.credentialFields ?? []
    }

    /// Effective values: saved base + dirty edits overlaid.
    private var effectiveLLMValues: [String: String] {
        var result = savedLLMValues
        for key in editedFields {
            result[key] = llmCredentialValues[key] ?? ""
        }
        return result
    }

    private var hasLLMCredentials: Bool {
        let required = currentLLMFields.filter { !$0.isOptional }
        let effective = effectiveLLMValues
        return required.allSatisfy { field in
            !(effective[field.key] ?? "").isEmpty
        }
    }

    // MARK: Body

    var body: some View {
        settingsGroupCard(L("LLM 文本处理", "LLM Settings"), icon: "gearshape.fill") {
            llmProviderPicker
            SettingsDivider()

            if selectedLLMProvider == .localQwen {
                localQwenStatusView
            } else if hasLLMCredentials && !isEditingLLM {
                credentialSummaryCard(rows: llmSummaryRows)
            } else {
                dynamicCredentialFields
            }

            HStack(spacing: 8) {
                Spacer()
                if selectedLLMProvider == .localQwen {
                    testButton(L("测试连接", "Test"), status: llmTestStatus) { testLLMConnection() }
                        .disabled(!LocalQwenLLMConfig.isModelAvailable)
                } else {
                    testButton(L("测试连接", "Test"), status: llmTestStatus) { testLLMConnection() }
                        .disabled(!hasLLMCredentials)
                    if hasLLMCredentials && !isEditingLLM {
                        secondaryButton(L("修改", "Edit")) {
                            testTask?.cancel()
                            llmTestStatus = .idle
                            llmCredentialValues = [:]
                            editedFields = []
                            isEditingLLM = true
                        }
                    } else {
                        if hasLLMCredentials && hasStoredLLM {
                            secondaryButton(L("取消", "Cancel")) {
                                testTask?.cancel()
                                llmTestStatus = .idle
                                loadLLMCredentials()
                            }
                        }
                        primaryButton(L("保存", "Save")) { saveLLMCredentials() }
                            .disabled(!hasLLMCredentials)
                    }
                }
            }
            .padding(.top, 12)
        }
        .task {
            loadLLMCredentials()
        }
    }

    // MARK: - Local Qwen Status

    private var localQwenStatusView: some View {
        let model = LocalQwenLLMConfig.availableModel
        let modelAvailable = model != nil
        return VStack(alignment: .leading, spacing: 8) {
            if let model {
                HStack(spacing: 6) {
                    HStack(spacing: 4) {
                        Text(model.displayName)
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(TF.settingsText)
                        Text("|")
                            .font(.system(size: 10))
                            .foregroundStyle(TF.settingsTextTertiary.opacity(0.5))
                        Text(L("~\(String(format: "%.1f", model.sizeGB))GB, Metal GPU 加速", "~\(String(format: "%.1f", model.sizeGB))GB, Metal GPU"))
                            .font(.system(size: 10))
                            .foregroundStyle(TF.settingsTextTertiary)
                    }
                    Spacer()
                    if serverStarting {
                        ProgressView().controlSize(.small)
                    } else if serverRunning {
                        HStack(spacing: 4) {
                            Image(systemName: "checkmark.circle.fill")
                                .font(.system(size: 10))
                                .foregroundStyle(TF.settingsAccentGreen)
                            Text(L("运行中", "Running"))
                                .font(.system(size: 10, weight: .medium))
                                .foregroundStyle(TF.settingsAccentGreen)
                        }
                        Button(L("停止", "Stop")) { stopLocalServer() }
                            .font(.system(size: 11, weight: .medium))
                            .buttonStyle(.borderedProminent)
                            .tint(TF.settingsAccentRed)
                            .controlSize(.small)
                    } else {
                        Button(L("启动", "Start")) { startLocalServer() }
                            .font(.system(size: 11, weight: .medium))
                            .buttonStyle(.borderedProminent)
                            .tint(TF.settingsAccentAmber)
                            .controlSize(.small)
                    }
                }
            } else {
                HStack(spacing: 8) {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(TF.settingsAccentRed)
                        .font(.system(size: 12))
                    Text(L("模型未找到，请将 GGUF 放到 sensevoice-server/models/", "Model not found, place GGUF in sensevoice-server/models/"))
                        .font(.system(size: 11))
                        .foregroundStyle(TF.settingsTextSecondary)
                }
            }
        }
        .padding(.vertical, 6)
        .task {
            await checkServerStatus()
        }
    }

    private func startLocalServer() {
        Task { await preloadLocalLLM() }
    }

    private func checkServerStatus() async {
        serverRunning = await SenseVoiceServerManager.shared.isRunning
    }

    /// Start server + send dummy request to trigger LLM model loading (~7-13s).
    private func preloadLocalLLM() async {
        serverStarting = true
        do {
            try await SenseVoiceServerManager.shared.start()

            let qPort = SenseVoiceServerManager.currentQwen3Port
            guard let port = qPort ?? SenseVoiceServerManager.currentPort else {
                NSLog("[Settings] No server port available for LLM")
                serverStarting = false
                return
            }

            // Re-enable LLM loading (in case it was disabled by stop button)
            if qPort != nil {
                let enableURL = URL(string: "http://127.0.0.1:\(port)/llm/load")!
                var enableReq = URLRequest(url: enableURL)
                enableReq.httpMethod = "POST"
                enableReq.timeoutInterval = 5
                _ = try? await URLSession.shared.data(for: enableReq)
            }

            // Trigger lazy LLM model load with a minimal request
            let url = URL(string: "http://127.0.0.1:\(port)/v1/chat/completions")!
            var request = URLRequest(url: url)
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.timeoutInterval = 60
            let body = #"{"messages":[{"role":"user","content":"hi"}],"max_tokens":1}"#
            request.httpBody = body.data(using: .utf8)
            NSLog("[Settings] Preloading local LLM model...")
            _ = try? await URLSession.shared.data(for: request)
            NSLog("[Settings] Local LLM model preloaded")
            serverRunning = true
        } catch {
            NSLog("[Settings] Local server start failed: %@", String(describing: error))
        }
        serverStarting = false
    }

    private func stopLocalServer() {
        Task {
            // Unload LLM model via HTTP to free GPU memory
            if let port = SenseVoiceServerManager.currentQwen3Port {
                let url = URL(string: "http://127.0.0.1:\(port)/llm/unload")!
                var request = URLRequest(url: url)
                request.httpMethod = "POST"
                request.timeoutInterval = 10
                _ = try? await URLSession.shared.data(for: request)
            }
            DebugFileLogger.log("LLM unloaded via /llm/unload")
            // Also stop the server process if ASR doesn't need it
            await stopServerIfUnneeded()
        }
    }

    /// Stop server if ASR doesn't need it (user switched away from local LLM).
    private func stopServerIfUnneeded() async {
        let asrNeedsServer = KeychainService.selectedASRProvider == .sherpa
        if !asrNeedsServer {
            await SenseVoiceServerManager.shared.stop()
            serverRunning = false
            NSLog("[Settings] Stopped local server (no longer needed)")
        }
    }

    // MARK: - Provider Picker

    private var llmProviderPicker: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(L("服务商", "Provider").uppercased())
                .font(.system(size: 10, weight: .semibold))
                .tracking(0.8)
                .foregroundStyle(TF.settingsTextTertiary)
            settingsDropdown(
                selection: Binding(
                    get: { selectedLLMProvider.rawValue },
                    set: { if let p = LLMProvider(rawValue: $0) { selectedLLMProvider = p } }
                ),
                options: LLMProvider.allCases.map { ($0.rawValue, $0.displayName) }
            )
        }
        .padding(.vertical, 6)
        .onChange(of: selectedLLMProvider) { _, newProvider in
            testTask?.cancel()
            llmTestStatus = .idle
            isEditingLLM = true
            loadLLMCredentialsForProvider(newProvider)

            // Auto-save provider switch if target already has credentials (or needs none)
            let oldProvider = KeychainService.selectedLLMProvider
            if newProvider == .localQwen || hasLLMCredentials {
                KeychainService.selectedLLMProvider = newProvider
                if newProvider == .localQwen {
                    Task { await preloadLocalLLM() }
                } else if oldProvider == .localQwen {
                    // Unload LLM model; only stop Qwen3 server if ASR doesn't need it
                    stopLocalServer()
                    let asrNeedsQwen3 = KeychainService.selectedASRProvider == .sherpa
                        && (UserDefaults.standard.object(forKey: "tf_qwen3FinalEnabled") as? Bool ?? true)
                    if !asrNeedsQwen3 {
                        Task { await SenseVoiceServerManager.shared.stopQwen3() }
                    }
                }
            }
        }
    }

    // MARK: - Credential Fields

    private var dynamicCredentialFields: some View {
        let fields = currentLLMFields
        let rows = stride(from: 0, to: fields.count, by: 2).map { i in
            Array(fields[i..<min(i+2, fields.count)])
        }
        return VStack(spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
                if index > 0 { SettingsDivider() }
                HStack(alignment: .top, spacing: 16) {
                    ForEach(row) { field in
                        credentialFieldRow(field)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    if row.count == 1 {
                        Spacer().frame(maxWidth: .infinity)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func credentialFieldRow(_ field: CredentialField) -> some View {
        let binding = Binding<String>(
            get: { llmCredentialValues[field.key] ?? "" },
            set: {
                llmCredentialValues[field.key] = $0
                editedFields.insert(field.key)
            }
        )
        if !field.options.isEmpty {
            let pickerBinding = Binding<String>(
                get: {
                    let val = llmCredentialValues[field.key] ?? ""
                    return val.isEmpty ? (savedLLMValues[field.key] ?? field.defaultValue) : val
                },
                set: {
                    llmCredentialValues[field.key] = $0
                    editedFields.insert(field.key)
                }
            )
            settingsPickerField(field.label, selection: pickerBinding, options: field.options)
        } else {
            let savedVal = savedLLMValues[field.key] ?? ""
            let placeholder = savedVal.isEmpty ? field.placeholder : maskedSecret(savedVal)
            if field.isSecure {
                settingsSecureField(field.label, text: binding, prompt: placeholder)
            } else {
                settingsField(field.label, text: binding, prompt: placeholder)
            }
        }
    }

    private var llmSummaryRows: [(String, String)] {
        var rows: [(String, String)] = []
        for field in currentLLMFields {
            let val = llmCredentialValues[field.key] ?? ""
            guard !val.isEmpty else { continue }
            let display = field.isSecure ? maskedSecret(val) : val
            rows.append((field.label, display))
        }
        return rows
    }

    // MARK: - Data

    private func loadLLMCredentials() {
        selectedLLMProvider = KeychainService.selectedLLMProvider
        loadLLMCredentialsForProvider(selectedLLMProvider)
    }

    private func loadLLMCredentialsForProvider(_ provider: LLMProvider) {
        testTask?.cancel()
        editedFields = []
        if let values = KeychainService.loadLLMCredentials(for: provider) {
            llmCredentialValues = values
            savedLLMValues = values
            hasStoredLLM = true
            isEditingLLM = !hasLLMCredentials
        } else {
            var defaults: [String: String] = [:]
            let fields = LLMProviderRegistry.configType(for: provider)?.credentialFields ?? []
            for field in fields where !field.defaultValue.isEmpty {
                defaults[field.key] = field.defaultValue
            }
            llmCredentialValues = defaults
            savedLLMValues = [:]
            hasStoredLLM = false
            isEditingLLM = true
        }
    }

    private func saveLLMCredentials() {
        let values = effectiveLLMValues
        do {
            try KeychainService.saveLLMCredentials(for: selectedLLMProvider, values: values)
            KeychainService.selectedLLMProvider = selectedLLMProvider
            llmCredentialValues = values
            savedLLMValues = values
            editedFields = []
            hasStoredLLM = true
            isEditingLLM = false
            llmTestStatus = .saved

            // Preload local LLM model on save
            if selectedLLMProvider == .localQwen {
                Task { await preloadLocalLLM() }
            }
        } catch {
            llmTestStatus = .failed(L("保存失败", "Save failed"))
        }
    }

    private func testLLMConnection() {
        testTask?.cancel()
        llmTestStatus = .testing
        let testValues = effectiveLLMValues
        let provider = selectedLLMProvider
        testTask = Task {
            do {
                let llmConfig: LLMConfig
                if provider == .localQwen {
                    // LLM runs on Qwen3-ASR server (shares Metal GPU lock)
                    let port = SenseVoiceServerManager.currentQwen3Port ?? SenseVoiceServerManager.currentPort
                    guard let port else {
                        guard !Task.isCancelled else { return }
                        llmTestStatus = .failed(L("Qwen3 服务未运行，请先启动", "Qwen3 server not running, start it first"))
                        return
                    }
                    llmConfig = LLMConfig(apiKey: "", model: "qwen3-4b", baseURL: "http://127.0.0.1:\(port)/v1")
                } else {
                    guard let configType = LLMProviderRegistry.configType(for: provider),
                          let config = configType.init(credentials: testValues)
                    else {
                        guard !Task.isCancelled else { return }
                        llmTestStatus = .failed(L("配置无效", "Invalid config"))
                        return
                    }
                    llmConfig = config.toLLMConfig()
                }
                let client: any LLMClient = provider == .claude
                    ? ClaudeChatClient()
                    : DoubaoChatClient(provider: provider)
                let reply = try await client.process(text: "hi", prompt: "{text}", config: llmConfig)
                guard !Task.isCancelled else { return }
                llmTestStatus = .success
                NSLog("[Settings] LLM test OK (%@): %@", provider.rawValue, reply)
            } catch {
                guard !Task.isCancelled else { return }
                NSLog("[Settings] LLM test failed (%@): %@", provider.rawValue, String(describing: error))
                llmTestStatus = .failed(error.localizedDescription)
            }
        }
    }
}
