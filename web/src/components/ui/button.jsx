import * as React from "react"
import { Button as RadixButton } from "@radix-ui/themes"

const Button = React.forwardRef(({ 
  variant = "solid", 
  size = "2", 
  color = "brown",
  className,
  ...props 
}, ref) => {
  return (
    <RadixButton
      ref={ref}
      variant={variant}
      size={size}
      color={color}
      {...props}
    />
  )
})
Button.displayName = "Button"

export { Button } 