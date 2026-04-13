import { useState } from 'react';
import { Settings, Mic, Brain, Cloud, Box, Info } from 'lucide-react';
import { GeneralTab } from './GeneralTab';
import { ASRSettingsCard } from './ASRSettingsCard';
import { LLMSettingsCard } from './LLMSettingsCard';
import { CloudSettingsCard } from './CloudSettingsCard';
import { ModelTab } from './ModelTab';
type TabId = 'general' | 'asr' | 'llm' | 'cloud' | 'models' | 'about';

interface Tab {
  id: TabId;
  label: string;
  icon: React.ReactNode;
}

const TABS: Tab[] = [
  { id: 'general', label: '通用', icon: <Settings size={18} /> },
  { id: 'asr', label: '语音识别', icon: <Mic size={18} /> },
  { id: 'llm', label: 'LLM 处理', icon: <Brain size={18} /> },
  { id: 'cloud', label: '云服务', icon: <Cloud size={18} /> },
  { id: 'models', label: '模型', icon: <Box size={18} /> },
  { id: 'about', label: '关于', icon: <Info size={18} /> },
];

export function SettingsWindow() {
  const [activeTab, setActiveTab] = useState<TabId>('general');

  return (
    <div className="flex h-screen bg-[var(--bg-primary)]">
      {/* Sidebar */}
      <nav className="w-48 shrink-0 border-r border-[var(--border)] bg-[var(--bg-secondary)] flex flex-col pt-6 pb-4">
        {/* Title */}
        <div className="px-5 mb-6" data-tauri-drag-region>
          <h1 className="text-lg font-semibold text-white tracking-tight">Type4Me</h1>
          <p className="text-[11px] text-gray-500 mt-0.5">设置</p>
        </div>

        {/* Tabs */}
        <div className="flex-1 space-y-0.5 px-2">
          {TABS.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm transition-colors ${
                activeTab === tab.id
                  ? 'bg-indigo-600/15 text-indigo-300'
                  : 'text-gray-400 hover:text-gray-200 hover:bg-white/5'
              }`}
            >
              {tab.icon}
              {tab.label}
            </button>
          ))}
        </div>
      </nav>

      {/* Content */}
      <main className="flex-1 overflow-y-auto">
        <div className="max-w-2xl mx-auto p-8">
          <TabContent tab={activeTab} />
        </div>
      </main>
    </div>
  );
}

function TabContent({ tab }: { tab: TabId }) {
  switch (tab) {
    case 'general':
      return <GeneralTab />;
    case 'asr':
      return <ASRSettingsCard />;
    case 'llm':
      return <LLMSettingsCard />;
    case 'cloud':
      return <CloudSettingsCard />;
    case 'models':
      return <ModelTab />;
    case 'about':
      return <AboutTab />;
  }
}

function AboutTab() {
  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-white mb-1">关于 Type4Me</h2>
        <p className="text-sm text-gray-400">语音输入工具 Windows 版</p>
      </div>

      <div className="p-4 bg-[var(--bg-secondary)] rounded-xl border border-[var(--border)] space-y-3">
        <div className="flex items-center justify-between text-sm">
          <span className="text-gray-400">版本</span>
          <span className="text-white font-mono">0.1.0</span>
        </div>
        <div className="flex items-center justify-between text-sm">
          <span className="text-gray-400">运行时</span>
          <span className="text-white font-mono">Tauri v2</span>
        </div>
        <div className="flex items-center justify-between text-sm">
          <span className="text-gray-400">平台</span>
          <span className="text-white font-mono">Windows</span>
        </div>
      </div>

      <div className="p-4 bg-[var(--bg-secondary)] rounded-xl border border-[var(--border)]">
        <p className="text-sm text-gray-400 leading-relaxed">
          Type4Me 是一款跨平台语音输入工具，支持多种 ASR 引擎和 LLM 后处理，
          让你在任何应用中都能用语音高效输入文字。
        </p>
      </div>
    </div>
  );
}
