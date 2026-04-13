import { useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { Check, Loader2, AlertCircle } from 'lucide-react';
import { useAppState } from '../../hooks/useAppState';
import { AudioVisualizer } from './AudioVisualizer';
import { TranscriptDisplay } from './TranscriptDisplay';

export function FloatingBar() {
  const { phase, segments, audioLevel, errorMessage, finalText, subscribe } = useAppState();

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    subscribe().then((unsub) => {
      cleanup = unsub;
    });
    return () => cleanup?.();
  }, [subscribe]);

  const isVisible = phase !== 'Hidden';

  return (
    <div className="w-full h-full flex items-end justify-center pb-4">
      <AnimatePresence>
        {isVisible && (
          <motion.div
            initial={{ y: 20, opacity: 0, scale: 0.95 }}
            animate={{ y: 0, opacity: 1, scale: 1 }}
            exit={{ y: 20, opacity: 0, scale: 0.95 }}
            transition={{ type: 'spring', damping: 25, stiffness: 300 }}
            className="flex items-center gap-3 px-4 py-2.5 rounded-full min-w-[320px] max-w-[480px]"
            style={{
              background: 'rgba(20, 20, 32, 0.92)',
              backdropFilter: 'blur(20px)',
              border: '1px solid rgba(99, 102, 241, 0.15)',
              boxShadow: '0 8px 32px rgba(0, 0, 0, 0.4), 0 0 0 1px rgba(99, 102, 241, 0.08)',
            }}
          >
            {/* Status indicator */}
            <StatusDot phase={phase} />

            {/* Content area */}
            <div className="flex-1 min-w-0 overflow-hidden">
              <BarContent
                phase={phase}
                segments={segments}
                errorMessage={errorMessage}
                finalText={finalText}
              />
            </div>

            {/* Audio visualizer (only during recording) */}
            {phase === 'Recording' && (
              <AudioVisualizer level={audioLevel} active={true} />
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function StatusDot({ phase }: { phase: string }) {
  const config = getDotConfig(phase);

  return (
    <div className="relative flex items-center justify-center w-5 h-5 shrink-0">
      {/* Pulse ring */}
      {config.pulse && (
        <motion.div
          className="absolute inset-0 rounded-full"
          style={{ background: config.color }}
          animate={{ scale: [1, 1.8, 1], opacity: [0.4, 0, 0.4] }}
          transition={{ duration: 1.5, repeat: Infinity, ease: 'easeInOut' }}
        />
      )}

      {/* Dot / icon */}
      {config.icon === 'dot' && (
        <div
          className="w-2.5 h-2.5 rounded-full relative z-10"
          style={{ background: config.color }}
        />
      )}
      {config.icon === 'spinner' && (
        <motion.div
          animate={{ rotate: 360 }}
          transition={{ duration: 1, repeat: Infinity, ease: 'linear' }}
        >
          <Loader2 size={16} className="text-yellow-400" />
        </motion.div>
      )}
      {config.icon === 'check' && (
        <motion.div
          initial={{ scale: 0 }}
          animate={{ scale: 1 }}
          transition={{ type: 'spring', damping: 15 }}
        >
          <Check size={16} className="text-emerald-400" />
        </motion.div>
      )}
      {config.icon === 'error' && (
        <AlertCircle size={16} className="text-red-400" />
      )}
    </div>
  );
}

function getDotConfig(phase: string) {
  switch (phase) {
    case 'Preparing':
      return { color: '#6366f1', pulse: true, icon: 'dot' as const };
    case 'Recording':
      return { color: '#22c55e', pulse: true, icon: 'dot' as const };
    case 'Processing':
      return { color: '#eab308', pulse: false, icon: 'spinner' as const };
    case 'Done':
      return { color: '#22c55e', pulse: false, icon: 'check' as const };
    case 'Error':
      return { color: '#ef4444', pulse: false, icon: 'error' as const };
    default:
      return { color: '#6366f1', pulse: false, icon: 'dot' as const };
  }
}

function BarContent({
  phase,
  segments,
  errorMessage,
  finalText,
}: {
  phase: string;
  segments: { id: string; text: string; is_confirmed: boolean }[];
  errorMessage: string | null;
  finalText: string | null;
}) {
  switch (phase) {
    case 'Preparing':
      return (
        <motion.span
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          className="text-[13px] text-indigo-300"
        >
          连接中...
        </motion.span>
      );

    case 'Recording':
      return <TranscriptDisplay segments={segments} />;

    case 'Processing':
      return (
        <motion.span
          initial={{ opacity: 0 }}
          animate={{ opacity: [0.5, 1, 0.5] }}
          transition={{ duration: 1.5, repeat: Infinity }}
          className="text-[13px] text-yellow-300"
        >
          处理中...
        </motion.span>
      );

    case 'Done':
      return (
        <motion.span
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          className="text-[13px] text-emerald-300 truncate block"
        >
          {finalText || '完成'}
        </motion.span>
      );

    case 'Error':
      return (
        <span className="text-[13px] text-red-300 truncate block">
          {errorMessage || '出错了'}
        </span>
      );

    default:
      return null;
  }
}
