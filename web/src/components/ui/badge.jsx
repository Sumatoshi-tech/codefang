import React from 'react'
import { Badge as RadixBadge } from '@radix-ui/themes'

const Badge = React.forwardRef(({ 
  variant = "solid", 
  color = "brown",
  size = "1",
  className,
  ...props 
}, ref) => {
  return (
    <RadixBadge
      ref={ref}
      variant={variant}
      color={color}
      size={size}
      {...props}
    />
  )
})
Badge.displayName = "Badge"

export { Badge } 