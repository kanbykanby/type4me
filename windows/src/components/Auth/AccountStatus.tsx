import { useEffect } from 'react';
import { User, LogOut, Zap } from 'lucide-react';
import { useAuth } from '../../hooks/useAuth';
import { useQuota } from '../../hooks/useQuota';

export function AccountStatus() {
  const { status, signOut } = useAuth();
  const { quota, refresh } = useQuota();

  useEffect(() => {
    refresh();
  }, [refresh]);

  if (!status?.is_logged_in) return null;

  const usedPercent = quota
    ? Math.max(0, Math.min(100, ((quota.total_chars - quota.free_chars_remaining) / Math.max(quota.total_chars, 1)) * 100))
    : 0;

  return (
    <div className="space-y-4">
      {/* User info */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-full bg-indigo-600/20 flex items-center justify-center">
            <User size={18} className="text-indigo-400" />
          </div>
          <div>
            <p className="text-sm text-white">{status.email}</p>
            <div className="flex items-center gap-1.5 mt-0.5">
              {quota?.is_paid ? (
                <span className="inline-flex items-center gap-1 px-2 py-0.5 text-[11px] font-medium bg-indigo-600/20 text-indigo-300 rounded-full">
                  <Zap size={10} /> Pro
                </span>
              ) : (
                <span className="inline-flex items-center px-2 py-0.5 text-[11px] font-medium bg-gray-700 text-gray-300 rounded-full">
                  Free
                </span>
              )}
            </div>
          </div>
        </div>
        <button
          onClick={signOut}
          className="flex items-center gap-1.5 px-3 py-1.5 text-sm text-gray-400 hover:text-red-400 hover:bg-red-400/10 rounded-lg transition-colors"
        >
          <LogOut size={14} />
          退出
        </button>
      </div>

      {/* Quota */}
      {quota && (
        <div className="space-y-2 p-3 bg-[var(--bg-tertiary)] rounded-lg">
          <div className="flex items-center justify-between text-sm">
            <span className="text-gray-400">用量配额</span>
            <span className="text-gray-300">
              {formatChars(quota.free_chars_remaining)} 剩余
            </span>
          </div>

          {/* Progress bar */}
          <div className="h-1.5 bg-gray-700 rounded-full overflow-hidden">
            <div
              className="h-full rounded-full transition-all duration-500"
              style={{
                width: `${usedPercent}%`,
                background: usedPercent > 80
                  ? 'linear-gradient(to right, #ef4444, #f97316)'
                  : 'linear-gradient(to right, #6366f1, #818cf8)',
              }}
            />
          </div>

          <div className="flex items-center justify-between text-xs text-gray-500">
            <span>本周已用 {formatChars(quota.week_chars)}</span>
            <span>累计 {formatChars(quota.total_chars)}</span>
          </div>
        </div>
      )}
    </div>
  );
}

function formatChars(n: number): string {
  if (n >= 10000) return `${(n / 10000).toFixed(1)}万`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return String(n);
}
