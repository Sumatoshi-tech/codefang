import React from 'react'
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels'
import { Button, Flex, Text, Box, Badge } from '@radix-ui/themes'
import { FileTextIcon, MagnifyingGlassIcon, CodeIcon, EyeOpenIcon, EyeNoneIcon } from '@radix-ui/react-icons'
import ReactJsonView from '@microlink/react-json-view'
import { LoadingSpinner } from './LoadingSpinner'

export function CodeOutputPanel({
  code,
  setCode,
  queryInput,
  setQueryInput,
  uastOutput,
  queryOutput,
  isParsing,
  isQuerying,
  showRawJson,
  setShowRawJson,
  onQueryChange,
  currentLanguage
}) {
  const quickQueries = [
    { label: 'Functions', query: 'filter(.roles has "Function")' },
    { label: 'Function Decls', query: 'filter(.type == "FunctionDecl")' },
    { label: 'All Children', query: 'map(.children)' },
    { label: 'Count Nodes', query: 'reduce(count)' },
    { label: 'Identifiers', query: 'filter(.type == "Identifier")' },
    { label: 'Comments', query: 'filter(.roles has "Comment")' },
    { label: 'Strings', query: 'filter(.type == "String")' },
    { label: 'Numbers', query: 'filter(.type == "Number")' }
  ]

  const handleQuickQuery = (query) => {
    onQueryChange(query)
  }

  return (
    <Box style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Code & Output Panel Header */}
      <Flex 
        justify="between" 
        align="center" 
        p="3" 
        style={{ 
          borderBottom: '1px solid var(--gray-6)',
          backgroundColor: 'var(--gray-1)'
        }}
      >
        <Text size="1" weight="medium">Code & Output</Text>
        <Button
          variant="outline"
          size="1"
          onClick={() => setShowRawJson(!showRawJson)}
        >
          {showRawJson ? <EyeNoneIcon /> : <EyeOpenIcon />}
          {showRawJson ? 'Tree' : 'Raw'}
        </Button>
      </Flex>

      {/* Main Content */}
      <PanelGroup direction="vertical" style={{ flex: 1, minHeight: 0 }}>
        {/* Code Input Panel */}
        <Panel defaultSize={40} minSize={20}>
          <Box style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
            <Flex 
              align="center" 
              gap="2" 
              p="3" 
              style={{ 
                borderBottom: '1px solid var(--gray-6)',
                backgroundColor: 'var(--gray-1)'
              }}
            >
              <FileTextIcon />
              <Text size="1" weight="medium">Code</Text>
              {isParsing && (
                <Badge color="brown" variant="soft">
                  <LoadingSpinner />
                  Parse
                </Badge>
              )}
            </Flex>
            <Box style={{ flex: 1, padding: '12px' }}>
              <textarea
                value={code}
                onChange={(e) => setCode(e.target.value)}
                placeholder="Enter code to parse..."
                style={{
                  width: '100%',
                  height: '100%',
                  padding: '8px',
                  fontSize: '12px',
                  fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Consolas, "Liberation Mono", Menlo, monospace',
                  backgroundColor: 'var(--color-surface)',
                  border: '1px solid var(--gray-6)',
                  borderRadius: '4px',
                  outline: 'none',
                  resize: 'none',
                  boxSizing: 'border-box',
                  color: 'var(--gray-12)',
                  overflow: 'auto'
                }}
                data-testid="code-editor"
              />
            </Box>
          </Box>
        </Panel>

        <PanelResizeHandle style={{ 
          height: '4px', 
          backgroundColor: 'var(--gray-6)',
          cursor: 'row-resize'
        }} />

        {/* Output Panel */}
        <Panel defaultSize={60} minSize={30}>
          <Box style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
            {/* Query Input - One line above output */}
            <Flex 
              align="center" 
              gap="2" 
              p="2" 
              style={{ 
                borderBottom: '1px solid var(--gray-6)',
                backgroundColor: 'var(--gray-1)',
                minHeight: '40px'
              }}
            >
              <MagnifyingGlassIcon />
              <Text size="1" weight="medium">Query:</Text>
              <input
                type="text"
                value={queryInput}
                onChange={(e) => onQueryChange(e.target.value)}
                placeholder="Enter UAST query..."
                style={{
                  flex: 1,
                  height: '24px',
                  padding: '4px 8px',
                  fontSize: '12px',
                  fontFamily: 'monospace',
                  backgroundColor: 'var(--color-surface)',
                  border: '1px solid var(--gray-6)',
                  borderRadius: '4px',
                  outline: 'none',
                  boxSizing: 'border-box'
                }}
                data-testid="query-input"
              />
              {isQuerying && (
                <Badge color="brown" variant="soft">
                  <LoadingSpinner />
                  Querying...
                </Badge>
              )}
            </Flex>

            {/* Quick Actions */}
            <Box 
              p="2" 
              style={{ 
                borderBottom: '1px solid var(--gray-6)',
                backgroundColor: 'var(--gray-1)'
              }}
            >
              <Box 
                style={{ 
                  display: 'grid',
                  gridTemplateColumns: 'repeat(auto-fit, minmax(80px, 1fr))',
                  gap: '4px',
                  maxWidth: '100%'
                }}
              >
                {quickQueries.map((quickQuery, index) => (
                  <Button
                    key={index}
                    variant="ghost"
                    size="1"
                    onClick={() => handleQuickQuery(quickQuery.query)}
                    style={{ 
                      fontSize: '10px',
                      padding: '2px 6px',
                      height: '20px',
                      whiteSpace: 'nowrap',
                      minWidth: '0',
                      overflow: 'hidden',
                      textOverflow: 'ellipsis'
                    }}
                    data-testid={`quick-query-${quickQuery.label.toLowerCase().replace(/\s+/g, '-')}`}
                  >
                    {quickQuery.label}
                  </Button>
                ))}
              </Box>
            </Box>

            {/* Output Header */}
            <Flex 
              align="center" 
              gap="2" 
              p="3" 
              style={{ 
                borderBottom: '1px solid var(--gray-6)',
                backgroundColor: 'var(--gray-1)'
              }}
            >
              <CodeIcon />
              <Text size="1" weight="medium">
                {queryOutput && queryInput && queryInput.trim() ? 'Query Results' : 'UAST Output'}
              </Text>
            </Flex>

            {/* Output Content */}
            <Box style={{ flex: 1, padding: '12px', overflow: 'auto' }}>
              <Box style={{ height: '100%' }} data-testid="uast-output">
                {showRawJson ? (
                  <pre style={{
                    fontSize: '12px',
                    fontFamily: 'monospace',
                    whiteSpace: 'pre-wrap',
                    height: '100%',
                    overflow: 'auto'
                  }}>
                    {queryOutput && queryInput && queryInput.trim() ? queryOutput : uastOutput}
                  </pre>
                ) : (
                  <Box style={{ height: '100%' }}>
                    {queryOutput && queryInput && queryInput.trim() ? (
                      // Display query results
                      <Box style={{ height: '100%' }}>
                        {queryOutput && 
                         queryOutput !== 'No UAST data available. Parse some code first.' && 
                         queryOutput !== 'No query results yet. Enter a query to see results.' &&
                         !queryOutput.startsWith('Query Error:') &&
                         !queryOutput.startsWith('Error executing query:') &&
                         !queryOutput.startsWith('Error parsing query results:') ? (
                          <ReactJsonView
                            src={JSON.parse(queryOutput)}
                            theme="monokai"
                            collapsed={2}
                            displayDataTypes={false}
                            displayObjectSize={true}
                            enableClipboard={true}
                            style={{ backgroundColor: 'transparent' }}
                          />
                        ) : (
                          <Flex 
                            align="center" 
                            justify="center" 
                            style={{ height: '100%' }}
                          >
                            <Text size="1" color={queryOutput.startsWith('Query Error:') || 
                                                  queryOutput.startsWith('Error executing query:') || 
                                                  queryOutput.startsWith('Error parsing query results:') ? 'red' : 'gray'}>
                              {queryOutput}
                            </Text>
                          </Flex>
                        )}
                      </Box>
                    ) : (
                      // Display UAST output
                      <Box style={{ height: '100%' }}>
                        {uastOutput && uastOutput !== 'No UAST data yet. Start typing code to parse automatically.' ? (
                          <ReactJsonView
                            src={JSON.parse(uastOutput)}
                            theme="monokai"
                            collapsed={2}
                            displayDataTypes={false}
                            displayObjectSize={true}
                            enableClipboard={true}
                            style={{ backgroundColor: 'transparent' }}
                          />
                        ) : (
                          <Flex 
                            align="center" 
                            justify="center" 
                            style={{ height: '100%' }}
                          >
                            <Text size="1" color="gray">{uastOutput}</Text>
                          </Flex>
                        )}
                      </Box>
                    )}
                  </Box>
                )}
              </Box>
            </Box>
          </Box>
        </Panel>
      </PanelGroup>
    </Box>
  )
} 