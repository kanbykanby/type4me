export type FloatingBarPhase = 'Hidden' | 'Preparing' | 'Recording' | 'Processing' | 'Done' | 'Error';

export interface TranscriptionSegment {
  id: string;
  text: string;
  is_confirmed: boolean;
}

export interface ProcessingMode {
  id: string;
  name: string;
  prompt: string;
  is_builtin: boolean;
  processing_label: string;
  hotkey_vk: number | null;
  hotkey_modifiers: number | null;
  hotkey_style: 'Hold' | 'Toggle';
}

export type ASRProvider = 'cloud' | 'volcano' | 'soniox' | 'deepgram' | 'eleven_labs' | 'openai' | 'sherpa';
export type LLMProvider = 'cloud' | 'openai' | 'claude' | 'deepseek' | 'doubao';

export interface ASRProviderInfo {
  id: ASRProvider;
  name: string;
  description: string;
  is_streaming: boolean;
  requires_credentials: boolean;
  credential_fields: CredentialField[];
}

export interface LLMProviderInfo {
  id: LLMProvider;
  name: string;
  description: string;
  requires_credentials: boolean;
  credential_fields: CredentialField[];
  default_model: string;
  available_models: string[];
}

export interface CredentialField {
  key: string;
  label: string;
  is_secure: boolean;
  placeholder: string;
}

export interface QuotaInfo {
  plan: string;
  is_paid: boolean;
  free_chars_remaining: number;
  week_chars: number;
  total_chars: number;
}

export interface AuthStatus {
  is_logged_in: boolean;
  email?: string;
  user_id?: string;
}

export interface AuthResult {
  success: boolean;
  token?: string;
  error?: string;
}

export interface ModelStatus {
  downloaded: boolean;
  size_mb: number;
  path?: string;
  model_name: string;
}
