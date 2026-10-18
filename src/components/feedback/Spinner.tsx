import React from 'react';

interface SpinnerProps {
  size?: number;
  color?: string;
  className?: string;
  label?: string;
}

export const Spinner: React.FC<SpinnerProps> = ({
  size = 14,
  color = 'var(--noesis-accent-blue, #89b4fa)',
  className = '',
  label = '加载中...',
}) => {
  return (
    <svg
      role="progressbar"
      aria-label={label}
      aria-busy="true"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={color}
      strokeWidth="2.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`spin ${className}`}
      style={{
        display: 'inline-block',
        verticalAlign: 'middle',
        flexShrink: 0,
      }}
    >
      <path d="M21 12a9 9 0 1 1-6.219-8.56" />
    </svg>
  );
};
