import React, { useState, useEffect } from 'react'
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels'
import { Flex, Box } from '@radix-ui/themes'
import { Header } from './components/Header'
import { UASTMappingEditor } from './components/UASTMappingEditor'
import { CodeOutputPanel } from './components/CodeOutputPanel'
import { ExamplesSidebar } from './components/ExamplesSidebar'
import { useUASTParser } from './hooks/useUASTParser'
import { useCustomMappings } from './hooks/useCustomMappings'
import { useEmbeddedMappings } from './hooks/useEmbeddedMappings'

function App() {
  const [showExamples, setShowExamples] = useState(false)
  const [showRawJson, setShowRawJson] = useState(false)

  // Initialize hooks
  const embeddedMappingsHook = useEmbeddedMappings()
  const {
    embeddedMappingsList,
    loadedMappings,
    isLoading: isLoadingEmbeddedMappings,
    error: embeddedMappingsError,
    getFormattedLanguages,
    loadMappingForLanguage
  } = embeddedMappingsHook

  const {
    code,
    setCode,
    queryInput,
    setQueryInput,
    uastOutput,
    queryOutput,
    isParsing,
    isQuerying,
    currentLanguage,
    setCurrentLanguage,
    selectedMapping,
    currentEmbeddedMapping,
    executeQuery,
    updateDefaultLanguage,
    selectEmbeddedMapping,
    selectCustomMapping,
    clearSelectedMapping,
    updateSelectedMapping,
    resetEmbeddedMapping
  } = useUASTParser(loadMappingForLanguage, embeddedMappingsHook)

  const {
    customMappings,
    setCustomMappings,
    addCustomMapping,
    removeCustomMapping,
    updateCustomMapping,
    getCustomMapping,
    getAllCustomMappings
  } = useCustomMappings()

  // Update default language when embedded mappings load
  useEffect(() => {
    if (!isLoadingEmbeddedMappings && embeddedMappingsList) {
      const availableLanguages = Object.keys(embeddedMappingsList)
      updateDefaultLanguage(availableLanguages)
    }
  }, [isLoadingEmbeddedMappings, embeddedMappingsList, updateDefaultLanguage])

  // Handle query execution
  const handleQueryChange = (newQuery) => {
    setQueryInput(newQuery)
    if (newQuery.trim()) {
      executeQuery(newQuery)
    }
  }

  // Handle mapping selection from examples
  const handleMappingSelect = (example) => {
    // Create a new mapping with the example content
    const newMapping = {
      name: example.title.toLowerCase().replace(/\s+/g, '_'),
      extensions: ['.ext'],
      uast: example.content
    }
    
    // Add the mapping to the list
    setCustomMappings([...customMappings, newMapping])
    
    // Automatically select the newly created mapping
    selectCustomMapping(newMapping)
  }

  return (
    <Box style={{ height: '100vh', display: 'flex', flexDirection: 'column' }}>
      {/* Header */}
      <Header
        language={currentLanguage}
        setLanguage={setCurrentLanguage}
        languages={getFormattedLanguages()}
        isParsing={isParsing}
        isQuerying={isQuerying}
        showExamples={showExamples}
        setShowExamples={setShowExamples}
        isLoadingLanguages={isLoadingEmbeddedMappings}
      />

      {/* Main Content */}
      <Box style={{ flex: 1, minHeight: 0 }}>
        <PanelGroup direction="horizontal" style={{ height: '100%' }}>
          {/* Left Panel - UAST Mapping Editor */}
          <Panel defaultSize={40} minSize={20}>
            <UASTMappingEditor
              customMappings={customMappings}
              embeddedMappingsList={embeddedMappingsList}
              isLoadingEmbeddedMappings={isLoadingEmbeddedMappings}
              currentLanguage={currentLanguage}
              currentEmbeddedMapping={currentEmbeddedMapping}
              selectedMapping={selectedMapping}
              addCustomMapping={addCustomMapping}
              removeCustomMapping={removeCustomMapping}
              updateCustomMapping={updateCustomMapping}
              selectEmbeddedMapping={selectEmbeddedMapping}
              selectCustomMapping={selectCustomMapping}
              clearSelectedMapping={clearSelectedMapping}
              updateSelectedMapping={updateSelectedMapping}
              resetEmbeddedMapping={resetEmbeddedMapping}
            />
          </Panel>

          {/* Resize Handle */}
          <PanelResizeHandle style={{ 
            width: '4px', 
            backgroundColor: 'var(--gray-6)',
            cursor: 'col-resize'
          }} />

          {/* Center Panel - Code & Output */}
          <Panel defaultSize={60} minSize={30}>
            <CodeOutputPanel
              code={code}
              setCode={setCode}
              queryInput={queryInput}
              setQueryInput={setQueryInput}
              uastOutput={uastOutput}
              queryOutput={queryOutput}
              isParsing={isParsing}
              isQuerying={isQuerying}
              showRawJson={showRawJson}
              setShowRawJson={setShowRawJson}
              onQueryChange={handleQueryChange}
              currentLanguage={currentLanguage}
            />
          </Panel>

          {/* Resize Handle */}
          <PanelResizeHandle style={{ 
            width: '4px', 
            backgroundColor: 'var(--gray-6)',
            cursor: 'col-resize'
          }} />

          {/* Right Panel - Examples Sidebar */}
          {showExamples && (
            <Panel defaultSize={20} minSize={15}>
              <ExamplesSidebar
                onMappingSelect={handleMappingSelect}
                onClose={() => setShowExamples(false)}
              />
            </Panel>
          )}
        </PanelGroup>
      </Box>
    </Box>
  )
}

export default App 