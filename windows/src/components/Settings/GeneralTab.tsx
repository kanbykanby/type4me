import { useEffect, useState, useCallback } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Plus, Trash2, GripVertical, Keyboard } from 'lucide-react';
import { useSettings } from '../../hooks/useSettings';
import type { ProcessingMode } from '../../lib/types';

export function GeneralTab() {
  const { modes, modesLoading, loadModes, updateModes } = useSettings();
  const [soundEnabled, setSoundEnabled] = useState(true);
  const [autoStart, setAutoStart] = useState(false);
  const [language, setLanguage] = useState('zh');

  useEffect(() => {
    loadModes();
  }, [loadModes]);

  const handleAddMode = useCallback(() => {
    const newMode: ProcessingMode = {
      id: crypto.randomUUID(),
      name: '新模式',
      prompt: '',
      is_builtin: false,
      processing_label: '处理中...',
      hotkey_vk: null,
      hotkey_modifiers: null,
      hotkey_style: 'Hold',
    };
    updateModes([...modes, newMode]);
  }, [modes, updateModes]);

  const handleDeleteMode = useCallback(
    (id: string) => {
      updateModes(modes.filter((m) => m.id !== id));
    },
    [modes, updateModes],
  );

  const handleUpdateMode = useCallback(
    (id: string, updates: Partial<ProcessingMode>) => {
      updateModes(modes.map((m) => (m.id === id ? { ...m, ...updates } : m)));
    },
    [modes, updateModes],
  );

  return (
    <div className="space-y-8">
      <div>
        <h2 className="text-xl font-semibold text-white mb-1">通用设置</h2>
        <p className="text-sm text-gray-400">快捷键、处理模式和基本偏好</p>
      </div>

      {/* Processing Modes */}
      <section className="space-y-3">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-medium text-gray-300">处理模式</h3>
          <button
            onClick={handleAddMode}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs text-indigo-300 bg-indigo-600/10 hover:bg-indigo-600/20 rounded-lg transition-colors"
          >
            <Plus size={14} />
            新增
          </button>
        </div>

        {modesLoading ? (
          <div className="text-sm text-gray-500 py-4 text-center">加载中...</div>
        ) : (
          <div className="space-y-2">
            <AnimatePresence>
              {modes.map((mode) => (
                <ModeItem
                  key={mode.id}
                  mode={mode}
                  onUpdate={(updates) => handleUpdateMode(mode.id, updates)}
                  onDelete={() => handleDeleteMode(mode.id)}
                />
              ))}
            </AnimatePresence>
          </div>
        )}
      </section>

      {/* Preferences */}
      <section className="space-y-3">
        <h3 className="text-sm font-medium text-gray-300">偏好设置</h3>

        <div className="space-y-1 p-4 bg-[var(--bg-secondary)] rounded-xl border border-[var(--border)]">
          {/* Sound toggle */}
          <div className="flex items-center justify-between py-2">
            <span className="text-sm text-gray-300">提示音</span>
            <ToggleSwitch checked={soundEnabled} onChange={setSoundEnabled} />
          </div>

          <div className="border-t border-[var(--border)]" />

          {/* Auto start */}
          <div className="flex items-center justify-between py-2">
            <span className="text-sm text-gray-300">开机自启</span>
            <ToggleSwitch checked={autoStart} onChange={setAutoStart} />
          </div>

          <div className="border-t border-[var(--border)]" />

          {/* Language */}
          <div className="flex items-center justify-between py-2">
            <span className="text-sm text-gray-300">界面语言</span>
            <select
              value={language}
              onChange={(e) => setLanguage(e.target.value)}
              className="bg-[var(--bg-tertiary)] border border-[var(--border)] text-sm text-white rounded-lg px-3 py-1.5 focus:outline-none focus:border-indigo-500"
            >
              <option value="zh">中文</option>
              <option value="en">English</option>
            </select>
          </div>
        </div>
      </section>
    </div>
  );
}

function ModeItem({
  mode,
  onUpdate,
  onDelete,
}: {
  mode: ProcessingMode;
  onUpdate: (updates: Partial<ProcessingMode>) => void;
  onDelete: () => void;
}) {
  const [expanded, setExpanded] = useState(false);

  return (
    <motion.div
      layout
      initial={{ opacity: 0, y: -8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -8 }}
      className="bg-[var(--bg-secondary)] border border-[var(--border)] rounded-xl overflow-hidden"
    >
      {/* Header */}
      <div
        className="flex items-center gap-2 px-3 py-2.5 cursor-pointer hover:bg-white/[0.02] transition-colors"
        onClick={() => setExpanded(!expanded)}
      >
        <GripVertical size={14} className="text-gray-600 shrink-0" />
        <span className="text-sm text-white flex-1">{mode.name}</span>
        {mode.is_builtin && (
          <span className="text-[10px] text-gray-500 px-1.5 py-0.5 bg-gray-800 rounded">内置</span>
        )}
        <div className="flex items-center gap-1 text-gray-500">
          <Keyboard size={12} />
          <span className="text-[11px]">
            {mode.hotkey_style === 'Hold' ? '按住' : '切换'}
          </span>
        </div>
      </div>

      {/* Expanded */}
      <AnimatePresence>
        {expanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="overflow-hidden"
          >
            <div className="px-4 pb-4 space-y-3 border-t border-[var(--border)] pt-3">
              {/* Name */}
              <div>
                <label className="block text-xs text-gray-500 mb-1">名称</label>
                <input
                  type="text"
                  value={mode.name}
                  onChange={(e) => onUpdate({ name: e.target.value })}
                  disabled={mode.is_builtin}
                  className="w-full px-3 py-2 bg-[var(--bg-tertiary)] border border-[var(--border)] rounded-lg text-sm text-white disabled:opacity-50 focus:outline-none focus:border-indigo-500"
                />
              </div>

              {/* Prompt */}
              <div>
                <label className="block text-xs text-gray-500 mb-1">Prompt</label>
                <textarea
                  value={mode.prompt}
                  onChange={(e) => onUpdate({ prompt: e.target.value })}
                  rows={3}
                  className="w-full px-3 py-2 bg-[var(--bg-tertiary)] border border-[var(--border)] rounded-lg text-sm text-white resize-none focus:outline-none focus:border-indigo-500"
                  placeholder="LLM 后处理指令..."
                />
              </div>

              {/* Hotkey style */}
              <div>
                <label className="block text-xs text-gray-500 mb-1">触发方式</label>
                <div className="flex gap-2">
                  {(['Hold', 'Toggle'] as const).map((style) => (
                    <button
                      key={style}
                      onClick={() => onUpdate({ hotkey_style: style })}
                      className={`flex-1 px-3 py-2 text-sm rounded-lg border transition-colors ${
                        mode.hotkey_style === style
                          ? 'border-indigo-500 bg-indigo-600/10 text-indigo-300'
                          : 'border-[var(--border)] text-gray-400 hover:bg-white/5'
                      }`}
                    >
                      {style === 'Hold' ? '按住录音' : '切换录音'}
                    </button>
                  ))}
                </div>
              </div>

              {/* Processing label */}
              <div>
                <label className="block text-xs text-gray-500 mb-1">处理提示文字</label>
                <input
                  type="text"
                  value={mode.processing_label}
                  onChange={(e) => onUpdate({ processing_label: e.target.value })}
                  className="w-full px-3 py-2 bg-[var(--bg-tertiary)] border border-[var(--border)] rounded-lg text-sm text-white focus:outline-none focus:border-indigo-500"
                />
              </div>

              {/* Delete */}
              {!mode.is_builtin && (
                <button
                  onClick={onDelete}
                  className="flex items-center gap-1.5 px-3 py-1.5 text-xs text-red-400 hover:bg-red-400/10 rounded-lg transition-colors"
                >
                  <Trash2 size={12} />
                  删除模式
                </button>
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
}

function ToggleSwitch({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      onClick={() => onChange(!checked)}
      className={`relative w-10 h-[22px] rounded-full transition-colors ${
        checked ? 'bg-indigo-600' : 'bg-gray-700'
      }`}
    >
      <motion.div
        className="absolute top-[2px] w-[18px] h-[18px] bg-white rounded-full shadow-sm"
        animate={{ left: checked ? 20 : 2 }}
        transition={{ type: 'spring', stiffness: 500, damping: 30 }}
      />
    </button>
  );
}
