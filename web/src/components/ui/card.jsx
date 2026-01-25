import * as React from "react"
import { Card as RadixCard, Box, Flex, Text } from "@radix-ui/themes"

const Card = React.forwardRef(({ className, ...props }, ref) => (
  <RadixCard
    ref={ref}
    {...props}
  />
))
Card.displayName = "Card"

const CardHeader = React.forwardRef(({ className, ...props }, ref) => (
  <Flex
    ref={ref}
    direction="column"
    gap="2"
    p="6"
    {...props}
  />
))
CardHeader.displayName = "CardHeader"

const CardTitle = React.forwardRef(({ className, ...props }, ref) => (
  <Text
    ref={ref}
    size="5"
    weight="bold"
    {...props}
  />
))
CardTitle.displayName = "CardTitle"

const CardDescription = React.forwardRef(({ className, ...props }, ref) => (
  <Text
    ref={ref}
    size="2"
    color="gray"
    {...props}
  />
))
CardDescription.displayName = "CardDescription"

const CardContent = React.forwardRef(({ className, ...props }, ref) => (
  <Box ref={ref} p="6" pt="0" {...props} />
))
CardContent.displayName = "CardContent"

const CardFooter = React.forwardRef(({ className, ...props }, ref) => (
  <Flex
    ref={ref}
    align="center"
    p="6"
    pt="0"
    {...props}
  />
))
CardFooter.displayName = "CardFooter"

export { Card, CardHeader, CardFooter, CardTitle, CardDescription, CardContent } 