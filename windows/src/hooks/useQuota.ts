import { create } from 'zustand';
import type { QuotaInfo } from '../lib/types';
import { quotaRefresh } from '../lib/tauri';

interface QuotaState {
  quota: QuotaInfo | null;
  loading: boolean;
  error: string | null;

  refresh: () => Promise<void>;
}

export const useQuota = create<QuotaState>((set) => ({
  quota: null,
  loading: false,
  error: null,

  refresh: async () => {
    set({ loading: true, error: null });
    try {
      const quota = await quotaRefresh();
      set({ quota, loading: false });
    } catch (err) {
      set({ loading: false, error: String(err) });
    }
  },
}));
