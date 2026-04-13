import { create } from 'zustand';
import type { FloatingBarPhase, TranscriptionSegment } from '../lib/types';
import {
  startRecording,
  stopRecording,
  cancelRecording,
  onBarPhaseChanged,
  onTranscriptUpdated,
  onAudioLevel,
  onSessionError,
  onSessionFinalized,
} from '../lib/tauri';

interface AppState {
  phase: FloatingBarPhase;
  segments: TranscriptionSegment[];
  audioLevel: number;
  errorMessage: string | null;
  finalText: string | null;

  setPhase: (phase: FloatingBarPhase) => void;
  setSegments: (segments: TranscriptionSegment[]) => void;
  setAudioLevel: (level: number) => void;
  setError: (msg: string | null) => void;
  setFinalText: (text: string | null) => void;

  doStartRecording: (modeId: string) => Promise<void>;
  doStopRecording: () => Promise<void>;
  doCancelRecording: () => Promise<void>;

  subscribe: () => Promise<() => void>;
}

export const useAppState = create<AppState>((set) => ({
  phase: 'Hidden',
  segments: [],
  audioLevel: 0,
  errorMessage: null,
  finalText: null,

  setPhase: (phase) => set({ phase }),
  setSegments: (segments) => set({ segments }),
  setAudioLevel: (level) => set({ audioLevel: level }),
  setError: (msg) => set({ errorMessage: msg }),
  setFinalText: (text) => set({ finalText: text }),

  doStartRecording: async (modeId: string) => {
    try {
      set({ errorMessage: null, segments: [], finalText: null });
      await startRecording(modeId);
    } catch (err) {
      set({ errorMessage: String(err), phase: 'Error' });
    }
  },

  doStopRecording: async () => {
    try {
      await stopRecording();
    } catch (err) {
      set({ errorMessage: String(err), phase: 'Error' });
    }
  },

  doCancelRecording: async () => {
    try {
      await cancelRecording();
      set({ phase: 'Hidden', segments: [], audioLevel: 0 });
    } catch (err) {
      set({ errorMessage: String(err) });
    }
  },

  subscribe: async () => {
    const unsubs = await Promise.all([
      onBarPhaseChanged((phase) => {
        set({ phase });
        if (phase === 'Hidden') {
          set({ segments: [], audioLevel: 0, errorMessage: null });
        }
      }),
      onTranscriptUpdated((segments) => {
        set({ segments });
      }),
      onAudioLevel((level) => {
        set({ audioLevel: level });
      }),
      onSessionError((msg) => {
        set({ errorMessage: msg, phase: 'Error' });
      }),
      onSessionFinalized((data) => {
        set({ finalText: data.text });
      }),
    ]);

    return () => {
      unsubs.forEach((fn) => fn());
    };
  },
}));
