import { useEffect, useRef } from 'react';
import type { TranscriptionSegment } from '../../lib/types';

interface TranscriptDisplayProps {
  segments: TranscriptionSegment[];
}

export function TranscriptDisplay({ segments }: TranscriptDisplayProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollLeft = containerRef.current.scrollWidth;
    }
  }, [segments]);

  if (segments.length === 0) {
    return (
      <span className="text-[13px] text-gray-500 italic">等待语音输入...</span>
    );
  }

  return (
    <div
      ref={containerRef}
      className="flex-1 overflow-x-auto whitespace-nowrap scrollbar-none text-[13px] leading-tight"
      style={{ scrollbarWidth: 'none', msOverflowStyle: 'none' }}
    >
      {segments.map((seg) => (
        <span
          key={seg.id}
          className={seg.is_confirmed ? 'text-white' : 'text-gray-400'}
        >
          {seg.text}
        </span>
      ))}
    </div>
  );
}
