import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AuthResult,
  AuthStatus,
  QuotaInfo,
  ProcessingMode,
  ASRProviderInfo,
  LLMProviderInfo,
  ModelStatus,
  FloatingBarPhase,
  TranscriptionSegment,
} from './types';

// Session
export const startRecording = (modeId: string) => invoke('start_recording', { modeId });
export const stopRecording = () => invoke('stop_recording');
export const cancelRecording = () => invoke('cancel_recording');

// Auth
export const authSendCode = (email: string) => invoke('auth_send_code', { email });
export const authVerify = (email: string, code: string) => invoke<AuthResult>('auth_verify', { email, code });
export const authSignOut = () => invoke('auth_sign_out');
export const authStatus = () => invoke<AuthStatus>('auth_status');

// Quota
export const quotaRefresh = () => invoke<QuotaInfo>('quota_refresh');
export const quotaCanUse = () => invoke<boolean>('quota_can_use');

// Modes
export const getModes = () => invoke<ProcessingMode[]>('get_modes');
export const saveModes = (modes: ProcessingMode[]) => invoke('save_modes', { modes });
export const getCurrentMode = () => invoke<ProcessingMode>('get_current_mode');

// ASR
export const getAsrProviders = () => invoke<ASRProviderInfo[]>('get_asr_providers');
export const getAsrProvider = () => invoke<string>('get_asr_provider');
export const setAsrProvider = (provider: string) => invoke('set_asr_provider', { provider });
export const getAsrCredentials = (provider: string) =>
  invoke<Record<string, string>>('get_asr_credentials', { provider });
export const saveAsrCredentials = (provider: string, credentials: Record<string, string>) =>
  invoke('save_asr_credentials', { provider, credentials });

// LLM
export const getLlmProviders = () => invoke<LLMProviderInfo[]>('get_llm_providers');
export const getLlmProvider = () => invoke<string>('get_llm_provider');
export const setLlmProvider = (provider: string) => invoke('set_llm_provider', { provider });
export const getLlmCredentials = (provider: string) =>
  invoke<Record<string, string>>('get_llm_credentials', { provider });
export const saveLlmCredentials = (provider: string, credentials: Record<string, string>) =>
  invoke('save_llm_credentials', { provider, credentials });

// Models
export const getModelStatus = () => invoke<ModelStatus>('get_model_status');
export const downloadModel = () => invoke('download_model');
export const deleteModel = () => invoke('delete_model');

// General
export const getAppEdition = () => invoke<string | null>('get_app_edition');
export const setAppEdition = (edition: string) => invoke('set_app_edition', { edition });
export const openSettingsWindow = () => invoke('open_settings_window');
export const getCloudRegion = () => invoke<string>('get_cloud_region');
export const setCloudRegion = (region: string) => invoke('set_cloud_region', { region });

// Event listeners
export const onBarPhaseChanged = (cb: (phase: FloatingBarPhase) => void): Promise<UnlistenFn> =>
  listen('bar-phase-changed', (e) => cb(e.payload as FloatingBarPhase));

export const onTranscriptUpdated = (cb: (segments: TranscriptionSegment[]) => void): Promise<UnlistenFn> =>
  listen('transcript-updated', (e) => cb(e.payload as TranscriptionSegment[]));

export const onAudioLevel = (cb: (level: number) => void): Promise<UnlistenFn> =>
  listen('audio-level', (e) => cb(e.payload as number));

export const onSessionError = (cb: (msg: string) => void): Promise<UnlistenFn> =>
  listen('session-error', (e) => cb(e.payload as string));

export const onSessionFinalized = (
  cb: (data: { text: string; outcome: string }) => void,
): Promise<UnlistenFn> =>
  listen('session-finalized', (e) => cb(e.payload as { text: string; outcome: string }));

export const onModelProgress = (cb: (data: { percent: number }) => void): Promise<UnlistenFn> =>
  listen('model-download-progress', (e) => cb(e.payload as { percent: number }));
