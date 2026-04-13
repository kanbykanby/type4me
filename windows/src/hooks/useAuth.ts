import { create } from 'zustand';
import type { AuthStatus } from '../lib/types';
import { authSendCode, authVerify, authSignOut, authStatus } from '../lib/tauri';

interface AuthState {
  status: AuthStatus | null;
  loading: boolean;
  error: string | null;
  codeSent: boolean;
  codeSending: boolean;
  verifying: boolean;

  checkStatus: () => Promise<void>;
  sendCode: (email: string) => Promise<boolean>;
  verify: (email: string, code: string) => Promise<boolean>;
  signOut: () => Promise<void>;
  clearError: () => void;
}

export const useAuth = create<AuthState>((set) => ({
  status: null,
  loading: false,
  error: null,
  codeSent: false,
  codeSending: false,
  verifying: false,

  checkStatus: async () => {
    set({ loading: true });
    try {
      const status = await authStatus();
      set({ status, loading: false });
    } catch (err) {
      set({ loading: false, error: String(err) });
    }
  },

  sendCode: async (email: string) => {
    set({ codeSending: true, error: null });
    try {
      await authSendCode(email);
      set({ codeSending: false, codeSent: true });
      return true;
    } catch (err) {
      set({ codeSending: false, error: String(err) });
      return false;
    }
  },

  verify: async (email: string, code: string) => {
    set({ verifying: true, error: null });
    try {
      const result = await authVerify(email, code);
      if (result.success) {
        const status = await authStatus();
        set({ verifying: false, status, codeSent: false });
        return true;
      } else {
        set({ verifying: false, error: result.error || '验证失败' });
        return false;
      }
    } catch (err) {
      set({ verifying: false, error: String(err) });
      return false;
    }
  },

  signOut: async () => {
    try {
      await authSignOut();
      set({ status: { is_logged_in: false }, codeSent: false });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  clearError: () => set({ error: null }),
}));
