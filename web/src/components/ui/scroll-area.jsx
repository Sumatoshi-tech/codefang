import * as React from "react"
import { ScrollArea as RadixScrollArea } from "@radix-ui/themes"

const ScrollArea = React.forwardRef(({ className, children, ...props }, ref) => (
  <RadixScrollArea
    ref={ref}
    {...props}
  >
    {children}
  </RadixScrollArea>
))

ScrollArea.displayName = "ScrollArea"

export { ScrollArea } 