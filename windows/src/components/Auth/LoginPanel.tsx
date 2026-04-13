import { useState, useEffect, useRef, useCallback } from 'react';
import { motion } from 'framer-motion';
import { Mail, ArrowRight, Loader2 } from 'lucide-react';
import { useAuth } from '../../hooks/useAuth';

export function LoginPanel() {
  const { codeSent, codeSending, verifying, error, sendCode, verify, clearError } = useAuth();
  const [email, setEmail] = useState('');
  const [code, setCode] = useState(['', '', '', '', '', '']);
  const [countdown, setCountdown] = useState(0);
  const codeRefs = useRef<(HTMLInputElement | null)[]>([]);

  useEffect(() => {
    if (countdown <= 0) return;
    const timer = setInterval(() => setCountdown((c) => c - 1), 1000);
    return () => clearInterval(timer);
  }, [countdown]);

  const handleSendCode = useCallback(async () => {
    if (!email.trim() || codeSending) return;
    clearError();
    const ok = await sendCode(email.trim());
    if (ok) {
      setCountdown(60);
      setTimeout(() => codeRefs.current[0]?.focus(), 100);
    }
  }, [email, codeSending, sendCode, clearError]);

  const handleCodeChange = useCallback(
    (index: number, value: string) => {
      if (value.length > 1) value = value.slice(-1);
      if (value && !/^\d$/.test(value)) return;

      const next = [...code];
      next[index] = value;
      setCode(next);

      if (value && index < 5) {
        codeRefs.current[index + 1]?.focus();
      }

      // Auto-submit when all 6 digits filled
      if (value && index === 5) {
        const fullCode = next.join('');
        if (fullCode.length === 6) {
          verify(email, fullCode);
        }
      }
    },
    [code, email, verify],
  );

  const handleCodeKeyDown = useCallback(
    (index: number, e: React.KeyboardEvent) => {
      if (e.key === 'Backspace' && !code[index] && index > 0) {
        codeRefs.current[index - 1]?.focus();
      }
      if (e.key === 'Enter') {
        const fullCode = code.join('');
        if (fullCode.length === 6) {
          verify(email, fullCode);
        }
      }
    },
    [code, email, verify],
  );

  const handleCodePaste = useCallback(
    (e: React.ClipboardEvent) => {
      e.preventDefault();
      const pasted = e.clipboardData.getData('text').replace(/\D/g, '').slice(0, 6);
      if (!pasted) return;
      const next = [...code];
      for (let i = 0; i < pasted.length; i++) {
        next[i] = pasted[i];
      }
      setCode(next);
      const focusIdx = Math.min(pasted.length, 5);
      codeRefs.current[focusIdx]?.focus();
      if (pasted.length === 6) {
        verify(email, pasted);
      }
    },
    [code, email, verify],
  );

  return (
    <div className="space-y-5">
      {/* Email input */}
      <div>
        <label className="block text-sm text-gray-400 mb-2">邮箱地址</label>
        <div className="flex gap-2">
          <div className="relative flex-1">
            <Mail size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-gray-500" />
            <input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleSendCode()}
              placeholder="you@example.com"
              className="w-full pl-10 pr-3 py-2.5 bg-[var(--bg-tertiary)] border border-[var(--border)] rounded-lg text-sm text-white placeholder:text-gray-600 focus:outline-none focus:border-indigo-500 transition-colors"
            />
          </div>
          <button
            onClick={handleSendCode}
            disabled={!email.trim() || codeSending || countdown > 0}
            className="px-4 py-2.5 bg-indigo-600 hover:bg-indigo-500 disabled:bg-gray-700 disabled:text-gray-500 text-white text-sm rounded-lg transition-colors flex items-center gap-2 shrink-0"
          >
            {codeSending ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              <ArrowRight size={14} />
            )}
            {countdown > 0 ? `${countdown}s` : '发送验证码'}
          </button>
        </div>
      </div>

      {/* Code input */}
      {codeSent && (
        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
        >
          <label className="block text-sm text-gray-400 mb-2">输入验证码</label>
          <div className="flex gap-2 justify-center" onPaste={handleCodePaste}>
            {code.map((digit, i) => (
              <input
                key={i}
                ref={(el) => { codeRefs.current[i] = el; }}
                type="text"
                inputMode="numeric"
                maxLength={1}
                value={digit}
                onChange={(e) => handleCodeChange(i, e.target.value)}
                onKeyDown={(e) => handleCodeKeyDown(i, e)}
                className="w-11 h-12 text-center text-lg font-mono bg-[var(--bg-tertiary)] border border-[var(--border)] rounded-lg text-white focus:outline-none focus:border-indigo-500 transition-colors"
              />
            ))}
          </div>

          {verifying && (
            <div className="flex items-center justify-center gap-2 mt-3 text-sm text-gray-400">
              <Loader2 size={14} className="animate-spin" />
              验证中...
            </div>
          )}
        </motion.div>
      )}

      {/* Error */}
      {error && (
        <motion.p
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          className="text-sm text-red-400 text-center"
        >
          {error}
        </motion.p>
      )}
    </div>
  );
}
