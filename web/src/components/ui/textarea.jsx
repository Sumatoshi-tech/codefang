import * as React from "react"
import { TextArea } from "@radix-ui/themes"

const Textarea = React.forwardRef(({ 
  size = "2", 
  className,
  ...props 
}, ref) => {
  return (
    <TextArea
      ref={ref}
      size={size}
      {...props}
    />
  )
})
Textarea.displayName = "Textarea"

export { Textarea } 