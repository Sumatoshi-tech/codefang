import React from 'react'

export function LoadingSpinner({ size = "sm" }) {
  const sizeMap = {
    sm: "12px",
    md: "16px", 
    lg: "24px"
  }
  
  return (
    <div 
      style={{
        width: sizeMap[size],
        height: sizeMap[size],
        border: '2px solid var(--brown-6)',
        borderTop: '2px solid var(--brown-9)',
        borderRadius: '50%',
        animation: 'loading-spinner-spin 1s linear infinite'
      }}
      role="status"
      aria-label="Loading"
    >
      <style>{`
        @keyframes loading-spinner-spin {
          0% { transform: rotate(0deg); }
          100% { transform: rotate(360deg); }
        }
      `}</style>
      <span style={{ display: 'none' }}>Loading...</span>
    </div>
  )
} 