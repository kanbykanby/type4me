import { useEffect, useState, useCallback } from 'react';
import { motion } from 'framer-motion';
import { Check, Loader2, Eye, EyeOff, Wifi, WifiOff } from 'lucide-react';
import { useSettings } from '../../hooks/useSettings';

export function ASRSettingsCard() {
  const {
    asrProviders,
    currentAsrProvider,
    asrCredentials,
    asrLoading,
    error,
    loadAsrProviders,
    selectAsrProvider,
    saveAsrCreds,
    clearError,
  } = useSettings();

  const [localCreds, setLocalCreds] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [visibleFields, setVisibleFields] = useState<Set<string>>(new Set());

  useEffect(() => {
    loadAsrProviders();
  }, [loadAsrProviders]);

  useEffect(() => {
    setLocalCreds(asrCredentials);
    setSaved(false);
  }, [asrCredentials]);

  const currentProviderInfo = asrProviders.find((p) => p.id === currentAsrProvider);

  const handleSave = useCallback(async () => {
    if (!currentAsrProvider) return;
    clearError();
    setSaving(true);
    await saveAsrCreds(currentAsrProvider, localCreds);
    setSaving(false);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  }, [currentAsrProvider, localCreds, saveAsrCreds, clearError]);

  const toggleFieldVisibility = useCallback((key: string) => {
    setVisibleFields((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const hasChanges = JSON.stringify(localCreds) !== JSON.stringify(asrCredentials);

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-white mb-1">语音识别</h2>
        <p className="text-sm text-gray-400">选择 ASR 引擎并配置凭证</p>
      </div>

      {/* Provider selection */}
      <div className="space-y-2">
        <label className="block text-sm text-gray-300">引擎</label>
        {asrLoading ? (
          <div className="py-4 text-center text-sm text-gray-500">加载中...</div>
        ) : (
          <div className="grid grid-cols-2 gap-2">
            {asrProviders.map((provider) => (
              <button
                key={provider.id}
                onClick={() => selectAsrProvider(provider.id)}
                className={`flex items-start gap-3 p-3 rounded-xl border text-left transition-colors ${
                  currentAsrProvider === provider.id
                    ? 'border-indigo-500 bg-indigo-600/8'
                    : 'border-[var(--border)] hover:border-gray-600 bg-[var(--bg-secondary)]'
                }`}
              >
                <div className="mt-0.5">
                  {provider.is_streaming ? (
                    <Wifi size={14} className="text-emerald-400" />
                  ) : (
                    <WifiOff size={14} className="text-gray-500" />
                  )}
                </div>
                <div className="flex-1 min-w-0">
                  <p className={`text-sm font-medium ${
                    currentAsrProvider === provider.id ? 'text-indigo-300' : 'text-white'
                  }`}>
                    {provider.name}
                  </p>
                  <p className="text-[11px] text-gray-500 mt-0.5 truncate">
                    {provider.description}
                  </p>
                </div>
              </button>
            ))}
          </div>
        )}
      </div>

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
