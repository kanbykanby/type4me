import { useEffect, useState, useCallback } from 'react';
import { motion } from 'framer-motion';
import { Download, Trash2, HardDrive, Loader2, CheckCircle2, AlertTriangle } from 'lucide-react';
import { getModelStatus, downloadModel, deleteModel, onModelProgress } from '../../lib/tauri';
import type { ModelStatus as ModelStatusType } from '../../lib/types';

export function ModelTab() {
  const [status, setStatus] = useState<ModelStatusType | null>(null);
  const [loading, setLoading] = useState(true);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [deleting, setDeleting] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const s = await getModelStatus();
      setStatus(s);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    onModelProgress(({ percent }) => {
      setProgress(percent);
      if (percent >= 100) {
        setDownloading(false);
        refresh();
      }
    }).then((unsub) => {
      cleanup = unsub;
    });
    return () => cleanup?.();
  }, [refresh]);

  const handleDownload = useCallback(async () => {
    setError(null);
    setDownloading(true);
    setProgress(0);
    try {
      await downloadModel();
    } catch (err) {
      setError(String(err));
      setDownloading(false);
    }
  }, []);

  const handleDelete = useCallback(async () => {
    if (!confirmDelete) {
      setConfirmDelete(true);
      return;
    }
    setDeleting(true);
    try {
      await deleteModel();
      setConfirmDelete(false);
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setDeleting(false);
    }
  }, [confirmDelete, refresh]);

  if (loading) {
    return (
      <div className="space-y-6">
        <div>
          <h2 className="text-xl font-semibold text-white mb-1">本地模型</h2>
          <p className="text-sm text-gray-400">管理本地语音识别模型</p>
        </div>
        <div className="py-8 text-center text-sm text-gray-500">加载中...</div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-white mb-1">本地模型</h2>
        <p className="text-sm text-gray-400">管理本地语音识别模型，离线可用</p>
      </div>

      {/* Model card */}
      {status && (
        <div className="p-5 bg-[var(--bg-secondary)] rounded-xl border border-[var(--border)] space-y-4">
          {/* Info */}
          <div className="flex items-start justify-between">
            <div className="flex items-center gap-3">
              <div className={`w-10 h-10 rounded-xl flex items-center justify-center ${
                status.downloaded
                  ? 'bg-emerald-600/15'
                  : 'bg-gray-700/50'
              }`}>
                <HardDrive size={20} className={status.downloaded ? 'text-emerald-400' : 'text-gray-500'} />
              </div>
              <div>
                <p className="text-sm font-medium text-white">{status.model_name}</p>
                <p className="text-xs text-gray-500 mt-0.5">{status.size_mb} MB</p>
              </div>
            </div>

            {/* Status badge */}
            {status.downloaded ? (
              <span className="flex items-center gap-1 px-2 py-1 text-[11px] text-emerald-300 bg-emerald-600/10 rounded-full">
                <CheckCircle2 size={12} />
                已安装
              </span>
            ) : (
              <span className="flex items-center gap-1 px-2 py-1 text-[11px] text-gray-400 bg-gray-700/50 rounded-full">
                未安装
              </span>
            )}
          </div>

          {/* Path */}
          {status.path && (
            <p className="text-[11px] text-gray-600 font-mono truncate">
              {status.path}
            </p>
          )}

          {/* Download progress */}
          {downloading && (
            <motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              className="space-y-2"
            >
              <div className="flex items-center justify-between text-xs">
                <span className="text-gray-400">下载中...</span>
                <span className="text-indigo-300 font-mono">{Math.round(progress)}%</span>
              </div>
              <div className="h-2 bg-gray-700 rounded-full overflow-hidden">
                <motion.div
                  className="h-full rounded-full"
                  style={{ background: 'linear-gradient(to right, #6366f1, #818cf8)' }}
                  initial={{ width: 0 }}
                  animate={{ width: `${progress}%` }}
                  transition={{ duration: 0.3 }}
                />
              </div>
            </motion.div>
          )}

          {/* Actions */}
          <div className="flex items-center gap-3 pt-1">
            {!status.downloaded && !downloading && (
              <button
                onClick={handleDownload}
                className="flex items-center gap-2 px-4 py-2.5 bg-indigo-600 hover:bg-indigo-500 text-white text-sm rounded-lg transition-colors"
              >
                <Download size={14} />
                下载模型
              </button>
            )}

            {downloading && (
              <button
                disabled
                className="flex items-center gap-2 px-4 py-2.5 bg-gray-700 text-gray-400 text-sm rounded-lg"
              >
                <Loader2 size={14} className="animate-spin" />
                下载中...
              </button>
            )}

            {status.downloaded && (
              <button
                onClick={handleDelete}
                className={`flex items-center gap-2 px-4 py-2.5 text-sm rounded-lg transition-colors ${
                  confirmDelete
                    ? 'bg-red-600 hover:bg-red-500 text-white'
                    : 'text-red-400 hover:bg-red-400/10'
                }`}
              >
                {deleting ? (
                  <Loader2 size={14} className="animate-spin" />
                ) : confirmDelete ? (
                  <AlertTriangle size={14} />
                ) : (
                  <Trash2 size={14} />
                )}
                {confirmDelete ? '确认删除' : '删除模型'}
              </button>
            )}

            {confirmDelete && (
              <button
                onClick={() => setConfirmDelete(false)}
                className="text-sm text-gray-400 hover:text-gray-200 transition-colors"
              >
                取消
              </button>
            )}
          </div>

          {/* Error */}
          {error && (
            <p className="text-xs text-red-400">{error}</p>
          )}
        </div>
      )}

      {/* Info text */}
      <div className="p-4 bg-[var(--bg-secondary)] rounded-xl border border-[var(--border)]">
        <p className="text-sm text-gray-400 leading-relaxed">
          本地模型支持完全离线的语音识别，无需网络连接。下载后即可在 ASR 设置中选择本地引擎。
          模型文件较大，请确保有足够的磁盘空间和稳定的网络连接。
        </p>
      </div>
    </div>
  );
}
