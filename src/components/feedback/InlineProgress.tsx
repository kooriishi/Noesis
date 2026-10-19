import React from 'react';

interface InlineProgressProps {
  height?: number;
  color?: string;
  className?: string;
}

export const InlineProgress: React.FC<InlineProgressProps> = ({
  height = 3,
  color = 'var(--noesis-accent-blue)',
  className = '',
}) => {
  return (
    <div
      role="progressbar"
      aria-label="进行中..."
      aria-busy="true"
      className={className}
      style={{
        position: 'relative',
        width: '100%',
        height: `${height}px`,
        backgroundColor: 'var(--noesis-bg-surface0)',
        overflow: 'hidden',
        borderRadius: 'var(--noesis-radius-sm, 4px)',
      }}
    >
      <div
        style={{
          position: 'absolute',
          top: 0,
          left: 0,
          bottom: 0,
          width: '40%',
          backgroundColor: color,
          borderRadius: 'var(--noesis-radius-sm, 4px)',
          animation: 'indeterminateProgress 1.4s cubic-bezier(0.65, 0.815, 0.735, 0.395) infinite',
        }}
      />
    </div>
  );
};
