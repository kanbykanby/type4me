
MacOS语音输入工具，本地/云端双引擎语音识别、大模型文本优化、全本地存储

<img width="420" height="78" alt="image" src="https://github.com/user-attachments/assets/dbc676e0-6128-4bed-89a2-553d2d1a197c" />


## 界面预览

<p align="center">
  <img src="https://github.com/user-attachments/assets/80b7e36d-92a4-40fb-84d6-d0b9da49bbcc" width="400" />
  <img src="https://github.com/user-attachments/assets/480df251-cd5f-462f-a574-ad0f5abd328a" width="400" />
</p>
<p align="center">
  <img src="https://github.com/user-attachments/assets/84a531be-b6d1-44e6-8dff-6763e9298ac1" width="400" />
  <img src="https://github.com/user-attachments/assets/ab2eecbb-62f1-4895-bd7c-49c138ef6da0" width="400" />
</p>



[查看演示视频](#演示视频)

### 下载

提供两个版本，功能完全相同，共享配置文件，可随时替换安装：

| 版本 | 说明 | 大小 |
|------|------|------|
| **[Type4Me-v1.5.0-local.dmg](https://github.com/joewongjc/type4me/releases/download/v1.5.0/Type4Me-v1.5.0-local.dmg)** | 内嵌 SenseVoice + Qwen3-ASR 本地识别，开箱即用 | ~1.2GB |
| **[Type4Me-v1.5.0-cloud.dmg](https://github.com/joewongjc/type4me/releases/download/v1.5.0/Type4Me-v1.5.0-cloud.dmg)** | 仅云端识别，需配置 API Key | ~23MB |

macOS 14+ (Sonoma)

> **首次打开提示安全警告？** 这是 macOS 对所有非 App Store 应用的正常行为，不影响使用。
>
> **方法一：通过系统设置（推荐）**
> 1. 双击打开 Type4Me.app，弹出安全提示后点击「完成」
> 2. 打开「系统设置」→「隐私与安全性」，滚动到底部「安全性」部分
> 3. 找到 "已阻止打开 Type4Me" 的提示，点击「仍要打开」
> 4. 输入密码确认，再次点击「打开」
>
> 只需操作一次，之后可正常启动。
>
> **方法二：通过终端**
> ```bash
> xattr -d com.apple.quarantine /Applications/Type4Me.app
> ```


## 为什么做 Type4Me
市面上语音输入法，至少命中以下问题之一：
贵（$12/月）、封闭（不可导出记录）、扩展性差（不能自定义Prompt）、慢。

## 功能亮点

### 本地语音识别 (SenseVoice)

基于阿里 [SenseVoice](https://github.com/FunAudioLLM/SenseVoice) 开源模型，**所有识别完全在设备端完成**，无需申请 API Key、无需注册云服务账号、无网络依赖。
- 两阶段识别：说话时实时显示文字，松手后自动校正提高准确率
- 支持热词加权（中文词和英文单词）
- 下载完整版 DMG 即可开箱使用，无需额外配置

### 云端流式识别（需自备API Key）
接入火山引擎（豆包）\OpenAi\Deepgram\AssemblyAI\Soniox\阿里云百炼\百度智能云，边说边出字。性能模式下还支持双通道识别，实时识别结束后用完整录音优化结果。
欢迎共建接入其他厂商的模型。（豆包现在注册送20-40小时识别，[配置指引](https://my.feishu.cn/wiki/QdEnwBMfUi0mN4k3ucMcNYhUnXr)）

### 自定义处理模式（需配置LLM API Key）

内置 5 种模式，也可以自定义任意多个：

| 模式 | 说明 |
|---|---|
| **快速模式** | 实时识别出文字，识别完成即输入，零延迟 |
| **性能模式** | 双通道识别，实时展示的体验 + 录音识别的准确|
| **英文翻译** | 说中文，输出英文翻译 |
| **Prompt优化** | 说一句简单的原始prompt，帮你优化后直接粘贴 |
| **命令模式** | 语音作为命令，结合选中文字和剪切板内容，让 LLM 执行操作 |
| **自定义** | 自己写 prompt，用 LLM 做任何后处理 |

每个模式可以绑定独立的全局快捷键，支持「按住说话」和「按一下开始/再按停止」两种方式。

注：已默认关闭思考模式，否则时间会很长。不同模型输出质量差异，我自己用的是seed-2.0-lite，大部分场景效果不错。

### Prompt 上下文变量

Prompt 模板支持三种变量，让语音输入从"听写"升级为"语音命令"：

| 变量 | 含义 |
|---|---|
| `{text}` | 语音识别的文字 |
| `{selected}` | 录音开始时光标选中的文字 |
| `{clipboard}` | 录音开始时剪切板的内容 |

**用法示例**：选中一段英文 → 按住快捷键说"翻译选中的文字" → LLM 收到选中内容 + 翻译指令，直接输出翻译结果。语音变成了 LLM 的命令，选中的文字和剪切板变成了上下文。

### 数据完全本地，支持导出

- 所有凭证存在本地文件 `~/Library/Application Support/Type4Me/credentials.json`（权限 0600），不经过任何中间服务器
- 识别历史记录存在本地 SQLite 数据库，支持按日期范围导出 CSV
- 无遥测、无数据上报、无云同步

### 词汇管理

- **ASR 热词**：添加专有名词（如 `Claude`、`Kubernetes`），提升识别准确率
- **片段替换**：语音说「我的邮箱」，自动替换为实际邮箱地址

### 提示音自定义

录音开始提示音支持多种选择：电子提示音、Water Drop 音效（两种）、或关闭。

### 更多特性

- 中英双语 UI，跟随系统语言自动切换
- 浮窗实时显示识别文本，带录音动画
- Dock 图标可在设置中显示/隐藏
- 首次使用有引导设置向导
- Swift Package Manager 构建
- 支持 macOS 14+

## 快速开始

### 方式一：直接下载（推荐）

下载上方 DMG 文件，拖入 Applications 即可使用。

- **完整版 (local)**：开箱即用，本地识别无需配置
- **云端版 (cloud)**：需要配置云端 ASR 的 API Key（如火山引擎）

### 方式二：从源码构建

#### 前置条件

- macOS 14.0 (Sonoma) 或更高版本
- Xcode Command Line Tools（`xcode-select --install`）
- CMake（`brew install cmake`，编译 SherpaOnnx 本地识别引擎需要）
- Python 3.12（`brew install python@3.12`，本地 SenseVoice 服务需要）

#### 第一步：克隆项目

```bash
git clone https://github.com/joewongjc/type4me.git
cd type4me
```

#### 第二步：编译本地识别引擎（约 5 分钟，仅需一次）

```bash
bash scripts/build-sherpa.sh
```

> 跳过这一步也能用，只是没有本地识别功能，云端引擎正常可用。

#### 第三步：搭建 SenseVoice 服务

```bash
cd sensevoice-server
python3.12 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

首次运行会自动从 ModelScope 下载 SenseVoice 模型（~900MB）。

#### 第四步：构建并部署

```bash
cd ..
bash scripts/deploy.sh
```

脚本会自动完成：编译 → 打包为 `.app` → 签名 → 安装到 `/Applications/` → 启动。

#### 第五步：配置

- **本地识别**：设置里选择「本地识别 (SenseVoice)」即可使用
- **云端识别**：首次启动会弹出设置向导，填入火山引擎的 App Key、Access Key 和 Resource ID。详见[配置指引](https://my.feishu.cn/wiki/QdEnwBMfUi0mN4k3ucMcNYhUnXr)

#### 后续更新

```bash
cd type4me
git pull
bash scripts/deploy.sh
```

## 架构概览

```
Type4Me/
├── ASR/                    # ASR 引擎抽象层
│   ├── ASRProvider.swift          # Provider 枚举 + 协议
│   ├── ASRProviderRegistry.swift  # 注册表
│   ├── Providers/                 # 各厂商配置
│   ├── SenseVoiceWSClient.swift   # 本地 SenseVoice WebSocket 客户端
│   ├── VolcASRClient.swift        # 火山引擎流式 ASR
│   └── DeepgramASRClient.swift    # Deepgram 流式 ASR
├── Bridge/                 # SherpaOnnx C API Swift 桥接
├── Audio/                  # 音频采集
├── Session/                # 核心状态机：录音 → ASR → 注入
├── Services/               # 凭证存储、热词、模型管理、SenseVoice 服务管理
├── Input/                  # 全局快捷键管理
└── UI/                     # SwiftUI 界面
    ├── FloatingBar/               # 浮窗
    └── Settings/                  # 设置界面
sensevoice-server/          # Python SenseVoice 流式 ASR 服务
├── server.py                      # FastAPI WebSocket 服务
└── sensevoice_model.py            # 流式推理引擎
```

ASR Provider 架构设计为可插拔：每个云厂商实现 `ASRProviderConfig`（定义凭证字段）和 `SpeechRecognizer`（实现识别逻辑），注册到 `ASRProviderRegistry` 即可。

## 参与贡献

**这个项目需要你的帮助。**

目前已经完成了 **火山引擎（Volcengine）** 和 **Deepgram** 的 ASR 适配。项目架构还预留了其他主流云厂商的接口定义（OpenAI Whisper、Google、AWS、Azure、阿里云、腾讯云、讯飞等），欢迎继续补齐客户端实现。

如果你在用其他语音识别服务，欢迎提交 PR 补充实现。添加一个新 Provider 只需要三步：

1. 在 `Type4Me/ASR/Providers/` 新建 Config 文件，实现 `ASRProviderConfig` 协议
2. 编写 ASR Client，实现 `SpeechRecognizer` 协议
3. 在 `ASRProviderRegistry.all` 中注册你的 `createClient`

当然，不只是 ASR Provider，任何形式的贡献都欢迎：

- 发现 bug？[提 Issue](../../issues)
- 有好想法？[开 Discussion](../../discussions)
- 想改代码？直接 Fork & PR

## 致谢

- [SenseVoice](https://github.com/FunAudioLLM/SenseVoice) - Alibaba FunAudioLLM
- [streaming-sensevoice](https://github.com/pengzhendong/streaming-sensevoice) - @pengzhendong
- [asr-decoder](https://github.com/pengzhendong/asr-decoder) - @pengzhendong
- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) - k2-fsa
- [FunASR](https://github.com/modelscope/FunASR) - Alibaba DAMO Academy

## For AI Agents

If you're an AI agent (Claude, Cursor, Copilot, etc.) tasked with building or deploying Type4Me, here's what you need to know:

**Read these files first:**

1. `CLAUDE.md` - architecture overview, credential storage, key files, and how to add new ASR providers
2. `Package.swift` - project dependencies and build targets
3. `scripts/deploy.sh` - the build & deploy pipeline

**Build & deploy steps:**

```bash
# 1. Clone
git clone https://github.com/joewongjc/type4me.git && cd type4me

# 2. Build local ASR engine (optional, ~5 min, needs cmake)
bash scripts/build-sherpa.sh

# 3. Setup SenseVoice server (optional, needs python3.12)
cd sensevoice-server && python3.12 -m venv .venv && source .venv/bin/activate && pip install -r requirements.txt && cd ..

# 4. Deploy as .app (builds, creates bundle, signs, and launches)
bash scripts/deploy.sh
```

**Key things to know:**

- This is a Swift Package Manager project, no `.xcodeproj` needed
- Local ASR uses a Python SenseVoice WebSocket service (`sensevoice-server/`), managed by `SenseVoiceServerManager`
- The ONNX int8 model is used for both streaming and full inference for best performance
- Credentials are stored locally at `~/Library/Application Support/Type4Me/credentials.json`, never in code or environment variables
- The ASR provider architecture is plugin-based: see `Type4Me/ASR/ASRProviderRegistry.swift`
- To add a new ASR provider, implement `ASRProviderConfig` + `SpeechRecognizer` protocol and register in `ASRProviderRegistry.all`

## 演示视频

<video src="https://github.com/user-attachments/assets/d5ad6da9-b924-4fd6-9812-d0d9868563a4" width="600" title="demo" controls>demo</video>




## 许可证

[MIT License](LICENSE)
