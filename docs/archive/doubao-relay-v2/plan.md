# 豆包输入法 + Type4Me 集成方案 v2：浮窗中转

## Context

豆包输入法的 ASR 质量优于 Type4Me 自带的火山引擎，但 Type4Me 有三个核心后处理功能需要保留：Snippet 关键词替换、LLM 后处理（翻译/润色）、历史记录。

**v1 方案（SIP hook）的问题**：直接 hook 豆包的 `insertText` 在目标应用做退格+替换，长文本退格不可靠，终端/微信兼容性差。

**v2 方案（浮窗中转）**：让豆包输入到 Type4Me 的浮窗文本框，Type4Me 拿到完整文本后走现有管道注入目标应用。等于把豆包当成一个"本地 ASR 引擎"。

## 核心架构

```
用户按 Type4Me 快捷键（如 Option+1 = 语音润色）
        │
        ▼
┌─────────────────────────────────────┐
│  1. 记录当前焦点应用                  │  NSWorkspace.frontmostApplication
│  2. 弹出 Type4Me 浮窗（带 NSTextView）│  激活窗口，抢焦点
│  3. 模拟豆包 ASR 快捷键（双击 RCtrl） │  豆包开始录音
└─────────────────────────────────────┘
        │
        ▼
   豆包流式 ASR → 文字输入到浮窗的 NSTextView
   （用户能看到流式文字，体验和豆包原生一样）
        │
        ▼
┌─────────────────────────────────────┐
│  检测 ASR 结束（豆包面板窗口消失）    │  复用 DoubaoASRObserver
│  或：用户按快捷键停止                 │
└─────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────┐
│  从 NSTextView 读取完整文本           │
│  ↓                                   │
│  Snippet 替换                        │  SnippetStorage.applyEffective()
│  ↓                                   │
│  LLM 处理（如果 mode 有 prompt）      │  现有 LLM 客户端
│  ↓                                   │
│  隐藏浮窗，切回目标应用               │  activate 记录的应用
│  ↓                                   │
│  注入文本                            │  TextInjectionEngine.inject()
│  ↓                                   │
│  记录历史                            │  HistoryStore.insert()
└─────────────────────────────────────┘
```

## 与 v1 的关键区别

| | v1 (hook) | v2 (浮窗中转) |
|---|---|---|
| 文本流向 | 豆包 → 目标应用 → 退格替换 | 豆包 → Type4Me 浮窗 → 处理 → 注入目标应用 |
| 需要 SIP off | 是（dylib 注入） | 否 |
| 退格/撤销 | 需要，长文本不可靠 | 不需要，一次粘贴 |
| 终端兼容性 | 差 | 好（Cmd+V 通用） |
| snippet hook | dylib 里做 | Type4Me 里做（现有代码） |
| LLM | 需要额外通知机制 | 直接复用 RecognitionSession 流程 |

## 实现步骤

### Step 1: DoubaoRelayPanel（新建）
**文件**: `Type4Me/Observer/DoubaoRelayPanel.swift`

一个**可激活的** NSPanel，内含 NSTextView，用于接收豆包的 IME 输入。

```swift
class DoubaoRelayPanel: NSPanel {
    let textView: NSTextView  // 接收豆包输入
    
    // 关键：必须是 activating panel（不是 .nonactivatingPanel）
    // 这样豆包 IME 才会把文字输入到这里
    
    // 外观：小巧的浮窗，显示在屏幕底部
    // 显示流式 ASR 文本（用户能看到识别过程）
}
```

关键配置：
- **不能**用 `.nonactivatingPanel`（现有 FloatingBarPanel 用的是这个，不接受 IME 输入）
- 需要 `.titled` + `.resizable` 或至少能成为 key window
- NSTextView 需要成为 first responder
- 浮窗级别 `.floating` 保持在最前

### Step 2: 修改 DoubaoIntegrationController
**文件**: `Type4Me/Observer/DoubaoIntegrationController.swift`

新增浮窗中转流程：

```swift
func startRelaySession(mode: ProcessingMode) {
    // 1. 记录当前焦点应用
    savedFrontApp = NSWorkspace.shared.frontmostApplication
    
    // 2. 显示浮窗 + 激活（获得焦点）
    relayPanel.show()
    relayPanel.makeKeyAndOrderFront(nil)
    NSApp.activate(ignoringOtherApps: true)
    relayPanel.textView.string = ""
    relayPanel.textView.window?.makeFirstResponder(relayPanel.textView)
    
    // 3. 触发豆包 ASR
    triggerDoubaoASR()
    
    // 4. 等待 ASR 结束（DoubaoASRObserver 检测面板消失）
}

func onASRComplete() {
    // 5. 读取文本
    let rawText = relayPanel.textView.string
    
    // 6. 隐藏浮窗
    relayPanel.hide()
    
    // 7. 切回目标应用
    savedFrontApp?.activate()
    
    // 8. 后处理 + 注入（复用现有代码）
    Task {
        let processed = SnippetStorage.applyEffective(to: rawText)
        var finalText = processed
        
        if let mode = armedMode, !mode.prompt.isEmpty {
            finalText = await llmClient.process(text: processed, ...)
        }
        
        // 注入到目标应用
        injectionEngine.inject(finalText)
        
        // 记录历史
        historyStore.insert(...)
    }
}
```

### Step 3: ASR 结束检测
复用 `DoubaoASRObserver` 的窗口检测逻辑（豆包 ASR 面板消失 = ASR 结束），或者监听 NSTextView 的文本变化（一段时间没有新输入 = ASR 结束）。

两种检测可以并用：
- **主要**：DoubaoASRObserver（面板消失）
- **备用**：NSTextView delegate 的 `textDidChange` + 1 秒无变化判定结束

### Step 4: 快捷键集成
复用现有 `registerHotkeys` 的豆包模式分支。按快捷键时调用 `startRelaySession(mode:)` 而不是之前的 `armLLMMode + triggerDoubaoASR`。

### Step 5: LLM 两步替换
哥的需求：翻译模式先显示"翻译中..."占位符，LLM 完成后替换为结果。

```swift
// Step 1: 注入占位符
injectionEngine.inject("翻译中...")

// Step 2: LLM 处理
let result = await llmClient.process(...)

// Step 3: 选中占位符 → 替换为结果
// 用 AX API 选中"翻译中..."的范围，再 inject(result)
```

### Step 6: 历史记录
直接复用 `HistoryStore.insert()`，asrProvider 标记为 "DoubaoIme"。

## 复用的现有代码

| 模块 | 文件 | 用途 |
|------|------|------|
| `SnippetStorage.applyEffective()` | Services/SnippetStorage.swift | Snippet 替换 |
| `TextInjectionEngine.inject()` | Injection/TextInjectionEngine.swift | 文本注入 |
| `HistoryStore.insert()` | Database/HistoryStore.swift | 历史记录 |
| `LLMClient.process()` | LLM/ClaudeChatClient.swift 等 | LLM 处理 |
| `PromptContext.capture()` | LLM/PromptContext.swift | prompt 变量展开 |
| `DoubaoASRObserver` | Observer/DoubaoASRObserver.swift | 豆包面板检测 |
| `ProcessingMode` | UI/AppState.swift | 模式定义 |
| `HotkeyManager` | Input/HotkeyManager.swift | 快捷键 |

## 需要注意的问题

1. **焦点切换闪烁**：弹出浮窗会短暂抢走焦点。可以用极小的浮窗或者半透明样式减少视觉干扰。
2. **豆包 ASR 面板位置**：豆包的小状态条会出现在浮窗附近（跟随光标），需要确保不遮挡。
3. **用户取消**：按 ESC 应该取消 ASR、隐藏浮窗、切回原应用、不注入。
4. **多次快速触发**：需要防止浮窗还没关就又打开。
5. **SIP hook 的清理**：v1 的 hook dylib 需要从 DoubaoIme 里移除，恢复原始二进制。

## 验证方案

1. **基础流程**：按快捷键 → 浮窗弹出 → 豆包语音 → 文字出现在浮窗 → ASR 结束 → 文字注入目标应用
2. **Snippet 替换**：说"克劳德"，注入结果应该是"Claude"
3. **LLM 处理**：用翻译模式，说中文，注入结果应该是英文
4. **历史记录**：在 Type4Me 设置的历史标签页里能看到记录
5. **多应用测试**：备忘录、微信、终端、浏览器
6. **取消测试**：ASR 进行中按 ESC，应该取消并恢复焦点
