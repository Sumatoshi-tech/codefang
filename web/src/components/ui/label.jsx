import * as React from "react"
import { Label as RadixLabel } from "@radix-ui/themes"

const Label = React.forwardRef(({ 
  size = "2", 
  className,
  ...props 
}, ref) => {
  return (
    <RadixLabel
      ref={ref}
      size={size}
      {...props}
    />
  )
})
Label.displayName = "Label"

export { Label } 