import React from 'react';

interface SkeletonProps {
  width?: string | number;
  height?: string | number;
  borderRadius?: string;
  className?: string;
  style?: React.CSSProperties;
}

export const Skeleton: React.FC<SkeletonProps> = ({
  width = '100%',
  height = '14px',
  borderRadius = 'var(--noesis-radius-sm, 4px)',
  className = '',
  style = {},
}) => {
  return (
    <div
      aria-hidden="true"
      className={className}
      style={{
        width: typeof width === 'number' ? `${width}px` : width,
        height: typeof height === 'number' ? `${height}px` : height,
        borderRadius,
        backgroundColor: 'var(--noesis-bg-surface0)',
        animation: 'skeletonPulse 1.6s ease-in-out infinite',
        ...style,
      }}
    />
  );
};
