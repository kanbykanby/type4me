import { create } from 'zustand';
import type { ASRProviderInfo, LLMProviderInfo, ProcessingMode } from '../lib/types';
import {
  getAsrProviders,
  getAsrProvider,
  setAsrProvider,
  getAsrCredentials,
  saveAsrCredentials,
  getLlmProviders,
  getLlmProvider,
  setLlmProvider,
  getLlmCredentials,
  saveLlmCredentials,
  getModes,
  saveModes,
} from '../lib/tauri';

interface SettingsState {
  // ASR
  asrProviders: ASRProviderInfo[];
  currentAsrProvider: string | null;
  asrCredentials: Record<string, string>;
  asrLoading: boolean;

  // LLM
  llmProviders: LLMProviderInfo[];
  currentLlmProvider: string | null;
  llmCredentials: Record<string, string>;
  llmLoading: boolean;

  // Modes
  modes: ProcessingMode[];
  modesLoading: boolean;

  error: string | null;

  // ASR actions
  loadAsrProviders: () => Promise<void>;
  selectAsrProvider: (provider: string) => Promise<void>;
  loadAsrCredentials: (provider: string) => Promise<void>;
  saveAsrCreds: (provider: string, creds: Record<string, string>) => Promise<void>;

  // LLM actions
  loadLlmProviders: () => Promise<void>;
  selectLlmProvider: (provider: string) => Promise<void>;
  loadLlmCredentials: (provider: string) => Promise<void>;
  saveLlmCreds: (provider: string, creds: Record<string, string>) => Promise<void>;

  // Mode actions
  loadModes: () => Promise<void>;
  updateModes: (modes: ProcessingMode[]) => Promise<void>;

  clearError: () => void;
}

export const useSettings = create<SettingsState>((set) => ({
  asrProviders: [],
  currentAsrProvider: null,
  asrCredentials: {},
  asrLoading: false,

  llmProviders: [],
  currentLlmProvider: null,
  llmCredentials: {},
  llmLoading: false,

  modes: [],
  modesLoading: false,

  error: null,

  // ASR
  loadAsrProviders: async () => {
    set({ asrLoading: true });
    try {
      const [providers, current] = await Promise.all([getAsrProviders(), getAsrProvider()]);
      set({ asrProviders: providers, currentAsrProvider: current, asrLoading: false });
      if (current) {
        const creds = await getAsrCredentials(current);
        set({ asrCredentials: creds });
      }
    } catch (err) {
      set({ asrLoading: false, error: String(err) });
    }
  },

  selectAsrProvider: async (provider: string) => {
    try {
      await setAsrProvider(provider);
      const creds = await getAsrCredentials(provider);
      set({ currentAsrProvider: provider, asrCredentials: creds });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  loadAsrCredentials: async (provider: string) => {
    try {
      const creds = await getAsrCredentials(provider);
      set({ asrCredentials: creds });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  saveAsrCreds: async (provider: string, creds: Record<string, string>) => {
    try {
      await saveAsrCredentials(provider, creds);
      set({ asrCredentials: creds });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  // LLM
  loadLlmProviders: async () => {
    set({ llmLoading: true });
    try {
      const [providers, current] = await Promise.all([getLlmProviders(), getLlmProvider()]);
      set({ llmProviders: providers, currentLlmProvider: current, llmLoading: false });
      if (current) {
        const creds = await getLlmCredentials(current);
        set({ llmCredentials: creds });
      }
    } catch (err) {
      set({ llmLoading: false, error: String(err) });
    }
  },

  selectLlmProvider: async (provider: string) => {
    try {
      await setLlmProvider(provider);
      const creds = await getLlmCredentials(provider);
      set({ currentLlmProvider: provider, llmCredentials: creds });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  loadLlmCredentials: async (provider: string) => {
    try {
      const creds = await getLlmCredentials(provider);
      set({ llmCredentials: creds });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  saveLlmCreds: async (provider: string, creds: Record<string, string>) => {
    try {
      await saveLlmCredentials(provider, creds);
      set({ llmCredentials: creds });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  // Modes
  loadModes: async () => {
    set({ modesLoading: true });
    try {
      const modes = await getModes();
      set({ modes, modesLoading: false });
    } catch (err) {
      set({ modesLoading: false, error: String(err) });
    }
  },

  updateModes: async (modes: ProcessingMode[]) => {
    try {
      await saveModes(modes);
      set({ modes });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  clearError: () => set({ error: null }),
}));
