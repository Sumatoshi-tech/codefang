import * as React from "react"
import { TextField } from "@radix-ui/themes"

const Input = React.forwardRef(({ 
  size = "2", 
  className,
  ...props 
}, ref) => {
  return (
    <TextField.Root
      ref={ref}
      size={size}
      {...props}
    />
  )
})
Input.displayName = "Input"

export { Input } 