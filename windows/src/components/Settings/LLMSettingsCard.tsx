import { useEffect, useState, useCallback } from 'react';
import { motion } from 'framer-motion';
import { Check, Loader2, Eye, EyeOff } from 'lucide-react';
import { useSettings } from '../../hooks/useSettings';

export function LLMSettingsCard() {
  const {
    llmProviders,
    currentLlmProvider,
    llmCredentials,
    llmLoading,
    error,
    loadLlmProviders,
    selectLlmProvider,
    saveLlmCreds,
    clearError,
  } = useSettings();

  const [localCreds, setLocalCreds] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [visibleFields, setVisibleFields] = useState<Set<string>>(new Set());

  useEffect(() => {
    loadLlmProviders();
  }, [loadLlmProviders]);

  useEffect(() => {
    setLocalCreds(llmCredentials);
    setSaved(false);
  }, [llmCredentials]);

  const currentProviderInfo = llmProviders.find((p) => p.id === currentLlmProvider);

  const handleSave = useCallback(async () => {
    if (!currentLlmProvider) return;
    clearError();
    setSaving(true);
    await saveLlmCreds(currentLlmProvider, localCreds);
    setSaving(false);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  }, [currentLlmProvider, localCreds, saveLlmCreds, clearError]);

  const toggleFieldVisibility = useCallback((key: string) => {
    setVisibleFields((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const hasChanges = JSON.stringify(localCreds) !== JSON.stringify(llmCredentials);

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-white mb-1">LLM 后处理</h2>
        <p className="text-sm text-gray-400">选择 LLM 引擎处理语音识别结果</p>
      </div>

      {/* Provider selection */}
      <div className="space-y-2">
        <label className="block text-sm text-gray-300">引擎</label>
        {llmLoading ? (
          <div className="py-4 text-center text-sm text-gray-500">加载中...</div>
        ) : (
          <div className="grid grid-cols-2 gap-2">
            {llmProviders.map((provider) => (
              <button
                key={provider.id}
                onClick={() => selectLlmProvider(provider.id)}
                className={`flex flex-col p-3 rounded-xl border text-left transition-colors ${
                  currentLlmProvider === provider.id
                    ? 'border-indigo-500 bg-indigo-600/8'
                    : 'border-[var(--border)] hover:border-gray-600 bg-[var(--bg-secondary)]'
                }`}
              >
                <p className={`text-sm font-medium ${
                  currentLlmProvider === provider.id ? 'text-indigo-300' : 'text-white'
                }`}>
                  {provider.name}
                </p>
                <p className="text-[11px] text-gray-500 mt-0.5 truncate">
                  {provider.description}
                </p>
                <p className="text-[10px] text-gray-600 mt-1 font-mono">
                  默认: {provider.default_model}
                </p>
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Model selection */}
      {currentProviderInfo && currentProviderInfo.available_models.length > 0 && (
        <div className="space-y-2">
          <label className="block text-sm text-gray-300">模型</label>
          <select
            value={localCreds['model'] ?? currentProviderInfo.default_model}
            onChange={(e) =>
              setLocalCreds((prev) => ({ ...prev, model: e.target.value }))
            }
            className="w-full px-3 py-2.5 bg-[var(--bg-secondary)] border border-[var(--border)] rounded-lg text-sm text-white focus:outline-none focus:border-indigo-500"
          >
            {currentProviderInfo.available_models.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
        </div>
      )}

      {/* Credentials */}
      {currentProviderInfo?.requires_credentials && (
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          className="space-y-4 p-4 bg-[var(--bg-secondary)] rounded-xl border border-[var(--border)]"
        >
          <h3 className="text-sm font-medium text-gray-300">凭证配置</h3>

          {currentProviderInfo.credential_fields.map((field) => (
            <div key={field.key}>
              <label className="block text-xs text-gray-500 mb-1">{field.label}</label>
              <div className="relative">
                <input
                  type={field.is_secure && !visibleFields.has(field.key) ? 'password' : 'text'}
                  value={localCreds[field.key] ?? ''}
                  onChange={(e) =>
                    setLocalCreds((prev) => ({ ...prev, [field.key]: e.target.value }))
                  }
                  placeholder={field.placeholder}
                  className="w-full px-3 py-2.5 pr-10 bg-[var(--bg-tertiary)] border border-[var(--border)] rounded-lg text-sm text-white font-mono placeholder:text-gray-600 focus:outline-none focus:border-indigo-500 transition-colors"
                />
                {field.is_secure && (
                  <button
                    type="button"
                    onClick={() => toggleFieldVisibility(field.key)}
                    className="absolute right-3 top-1/2 -translate-y-1/2 text-gray-500 hover:text-gray-300"
                  >
                    {visibleFields.has(field.key) ? <EyeOff size={14} /> : <Eye size={14} />}
                  </button>
                )}
              </div>
            </div>
          ))}

          {/* Save button */}
          <div className="flex items-center gap-3 pt-1">
            <button
              onClick={handleSave}
              disabled={saving || !hasChanges}
              className="flex items-center gap-2 px-4 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:bg-gray-700 disabled:text-gray-500 text-white text-sm rounded-lg transition-colors"
            >
              {saving ? (
                <Loader2 size={14} className="animate-spin" />
              ) : saved ? (
                <Check size={14} />
              ) : null}
              {saved ? '已保存' : '保存'}
            </button>

            {error && (
              <span className="text-xs text-red-400">{error}</span>
            )}
          </div>
        </motion.div>
      )}
    </div>
  );
}
