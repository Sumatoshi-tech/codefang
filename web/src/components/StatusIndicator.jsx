import React from 'react'
import { Flex, Text, Badge } from '@radix-ui/themes'
import { LoadingSpinner } from './LoadingSpinner'

export function StatusIndicator({ 
  status = 'idle', 
  message, 
  size = 'sm'
}) {
  const statusConfig = {
    idle: {
      icon: null,
      color: 'gray',
      variant: 'soft'
    },
    loading: {
      icon: <LoadingSpinner size={size} />,
      color: 'brown',
      variant: 'soft'
    },
    success: {
      icon: <div style={{ width: '12px', height: '12px', borderRadius: '50%', backgroundColor: 'var(--green-9)' }} />,
      color: 'green',
      variant: 'soft'
    },
    error: {
      icon: <div style={{ width: '12px', height: '12px', borderRadius: '50%', backgroundColor: 'var(--red-9)' }} />,
      color: 'red',
      variant: 'soft'
    },
    warning: {
      icon: <div style={{ width: '12px', height: '12px', borderRadius: '50%', backgroundColor: 'var(--yellow-9)' }} />,
      color: 'yellow',
      variant: 'soft'
    }
  }

  const config = statusConfig[status] || statusConfig.idle

  return (
    <Badge color={config.color} variant={config.variant}>
      <Flex align="center" gap="2">
        {config.icon}
        {message && <Text size="1">{message}</Text>}
      </Flex>
    </Badge>
  )
} 