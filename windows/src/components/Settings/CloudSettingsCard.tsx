import { useEffect, useState, useCallback } from 'react';
import { Globe } from 'lucide-react';
import { useAuth } from '../../hooks/useAuth';
import { LoginPanel } from '../Auth/LoginPanel';
import { AccountStatus } from '../Auth/AccountStatus';
import { getCloudRegion, setCloudRegion } from '../../lib/tauri';

export function CloudSettingsCard() {
  const { status, loading, checkStatus } = useAuth();
  const [region, setRegionLocal] = useState<string>('cn');

  useEffect(() => {
    checkStatus();
    getCloudRegion().then(setRegionLocal).catch(() => {});
  }, [checkStatus]);

  const handleRegionChange = useCallback(async (r: string) => {
    setRegionLocal(r);
    try {
      await setCloudRegion(r);
    } catch {
      // silently fail
    }
  }, []);

  if (loading) {
    return (
      <div className="space-y-6">
        <div>
          <h2 className="text-xl font-semibold text-white mb-1">云服务</h2>
          <p className="text-sm text-gray-400">登录账号使用云端功能</p>
        </div>
        <div className="py-8 text-center text-sm text-gray-500">加载中...</div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-white mb-1">云服务</h2>
        <p className="text-sm text-gray-400">登录账号使用云端功能</p>
      </div>

      {/* Auth section */}
      <div className="p-4 bg-[var(--bg-secondary)] rounded-xl border border-[var(--border)]">
        {status?.is_logged_in ? <AccountStatus /> : <LoginPanel />}
      </div>

      {/* Region */}
      <div className="p-4 bg-[var(--bg-secondary)] rounded-xl border border-[var(--border)] space-y-3">
        <div className="flex items-center gap-2">
          <Globe size={16} className="text-gray-400" />
          <h3 className="text-sm font-medium text-gray-300">服务区域</h3>
        </div>

        <div className="flex gap-2">
          {[
            { id: 'cn', label: '中国大陆', desc: '低延迟' },
            { id: 'overseas', label: '海外', desc: 'Global' },
          ].map((r) => (
            <button
              key={r.id}
              onClick={() => handleRegionChange(r.id)}
              className={`flex-1 p-3 rounded-xl border text-left transition-colors ${
                region === r.id
                  ? 'border-indigo-500 bg-indigo-600/8'
                  : 'border-[var(--border)] hover:border-gray-600'
              }`}
            >
              <p className={`text-sm font-medium ${
                region === r.id ? 'text-indigo-300' : 'text-white'
              }`}>
                {r.label}
              </p>
              <p className="text-[11px] text-gray-500 mt-0.5">{r.desc}</p>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
