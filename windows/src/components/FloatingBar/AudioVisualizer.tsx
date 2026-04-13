import { useMemo } from 'react';

interface AudioVisualizerProps {
  level: number; // 0..1
  active: boolean;
}

const SENSITIVITY = [0.6, 0.85, 1.0, 0.9, 0.7];
const MIN_HEIGHT = 3;
const MAX_HEIGHT = 20;

export function AudioVisualizer({ level, active }: AudioVisualizerProps) {
  const heights = useMemo(() => {
    return SENSITIVITY.map((s) => {
      if (!active) return MIN_HEIGHT;
      const h = MIN_HEIGHT + (MAX_HEIGHT - MIN_HEIGHT) * level * s;
      return Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, h));
    });
  }, [level, active]);

  return (
    <div className="flex items-center gap-[2px] h-5">
      {heights.map((h, i) => (
        <div
          key={i}
          className="w-[3px] rounded-full transition-all duration-100 ease-out"
          style={{
            height: `${h}px`,
            background: active
              ? `linear-gradient(to top, #6366f1, #818cf8)`
              : '#3f3f5e',
          }}
        />
      ))}
    </div>
  );
}
