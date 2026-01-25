import React from 'react'
import { Button, Select, Flex, Text, Badge } from '@radix-ui/themes'
import { BookmarkIcon } from '@radix-ui/react-icons'
import { LoadingSpinner } from './LoadingSpinner'
import { ThemeToggle } from './ThemeToggle'
import { StatusIndicator } from './StatusIndicator'

export function Header({ 
  language, 
  setLanguage, 
  languages, 
  isParsing, 
  isQuerying, 
  showExamples, 
  setShowExamples,
  isLoadingLanguages 
}) {
  return (
    <Flex 
      justify="between" 
      align="center" 
      p="3" 
      style={{ 
        borderBottom: '1px solid var(--gray-6)',
        backgroundColor: 'var(--gray-1)'
      }}
    >
      {/* Left side - Language selector */}
      <Flex align="center" gap="2">
        <Text size="1" color="gray">Lang:</Text>
        {isLoadingLanguages ? (
          <Flex align="center" gap="1" data-testid="loading-indicator">
            <LoadingSpinner />
            <Text size="1" color="gray">...</Text>
          </Flex>
        ) : (
          <Select.Root value={language} onValueChange={setLanguage}>
            <Select.Trigger size="1" style={{ width: '80px' }} data-testid="language-selector" />
            <Select.Content data-testid="language-dropdown">
              {languages.map((lang) => (
                <Select.Item 
                  key={lang.value} 
                  value={lang.value}
                  data-testid={`language-option-${lang.value}`}
                >
                  {lang.label}
                </Select.Item>
              ))}
            </Select.Content>
          </Select.Root>
        )}
      </Flex>

      {/* Right side - Status and controls */}
      <Flex align="center" gap="2">
        {/* Status indicators */}
        <Flex align="center" gap="1">
          {isParsing && (
            <Badge color="brown" variant="soft">
              <LoadingSpinner />
              Parse
            </Badge>
          )}
          {isQuerying && (
            <Badge color="brown" variant="soft">
              <LoadingSpinner />
              Query
            </Badge>
          )}
        </Flex>

        {/* Theme toggle */}
        <ThemeToggle />

        {/* Examples toggle */}
        <Button
          variant={showExamples ? "solid" : "outline"}
          size="1"
          onClick={() => setShowExamples(!showExamples)}
        >
          <BookmarkIcon />
          Examples
        </Button>
      </Flex>
    </Flex>
  )
} 