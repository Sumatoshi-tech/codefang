import React from 'react'
import { Button, TextField, Flex, Text, Box, Badge, Card } from '@radix-ui/themes'
import { GearIcon, PlusIcon, TrashIcon, CheckIcon, ResetIcon } from '@radix-ui/react-icons'
import { LoadingSpinner } from './LoadingSpinner'

export function UASTMappingEditor({ 
  customMappings, 
  embeddedMappingsList,
  isLoadingEmbeddedMappings,
  currentLanguage,
  currentEmbeddedMapping,
  selectedMapping,
  addCustomMapping, 
  removeCustomMapping, 
  updateCustomMapping,
  selectEmbeddedMapping,
  selectCustomMapping,
  clearSelectedMapping,
  updateSelectedMapping,
  resetEmbeddedMapping
}) {
  const hasEmbeddedMapping = embeddedMappingsList && currentLanguage && embeddedMappingsList[currentLanguage]
  const embeddedMappingLoaded = currentEmbeddedMapping !== null
  const isEmbeddedSelected = selectedMapping?.isEmbedded === true
  const isCustomizedEmbedded = selectedMapping?.isCustomized === true

  // Debug logging
  console.log('UASTMappingEditor state:', {
    currentLanguage,
    hasEmbeddedMapping,
    embeddedMappingsList: embeddedMappingsList ? Object.keys(embeddedMappingsList) : null,
    selectedMapping: selectedMapping?.name,
    customMappingsCount: customMappings.length
  })

  return (
    <Box style={{ display: 'flex', flexDirection: 'column', height: '100%' }} data-testid="mapping-editor">
      {/* UAST Mapping Editor Header */}
      <Flex 
        justify="between" 
        align="center" 
        p="3" 
        style={{ 
          borderBottom: '1px solid var(--gray-6)',
          backgroundColor: 'var(--gray-1)'
        }}
      >
        <Text size="1" weight="medium">UAST Mapping Editor</Text>
        <Flex gap="2">
          <Button
            variant="outline"
            size="1"
            onClick={() => {
              const newMapping = addCustomMapping()
              selectCustomMapping(newMapping)
            }}
            data-testid="create-custom-button"
          >
            <PlusIcon />
            New Custom
          </Button>
        </Flex>
      </Flex>
      
      {/* UAST Mapping Content */}
      <Box style={{ flex: 1, padding: '12px' }}>
        {isLoadingEmbeddedMappings ? (
          <Flex 
            direction="column" 
            align="center" 
            justify="center" 
            style={{ height: '100%', textAlign: 'center' }}
          >
            <Flex align="center" gap="2" color="gray">
              <LoadingSpinner size="md" />
              <Text size="1" color="gray">Loading embedded mappings...</Text>
            </Flex>
          </Flex>
        ) : !selectedMapping ? (
          <Flex 
            direction="column" 
            align="center" 
            justify="center" 
            style={{ height: '100%', textAlign: 'center' }}
          >
            <Flex direction="column" align="center" gap="3" color="gray">
              <GearIcon style={{ width: '32px', height: '32px', opacity: 0.5 }} />
              <Text size="2" weight="medium">No UAST Mapping Selected</Text>
              <Text size="1" color="gray" data-testid="no-mapping-message">
                {!currentLanguage 
                  ? 'Select a language first, then choose an embedded mapping or create a custom one'
                  : hasEmbeddedMapping 
                    ? `Select the embedded mapping for ${currentLanguage} or create a custom one using the buttons above`
                    : `No embedded mapping available for ${currentLanguage}. Create a custom mapping using the button above.`
                }
              </Text>
            </Flex>
          </Flex>
        ) : (
          <Flex direction="column" style={{ height: '100%' }} gap="3">
            {/* Mapping Selection */}
            <Box data-testid="mapping-selection">
              <Flex align="center" gap="2" mb="2">
                <Text size="1" weight="medium" color="gray">Available Mappings:</Text>
              </Flex>
              
              {/* Embedded Mapping Option */}
              {hasEmbeddedMapping && (
                <Flex align="center" gap="2" mb="1">
                  <Button
                    variant={isEmbeddedSelected ? "solid" : "outline"}
                    size="1"
                    onClick={selectEmbeddedMapping}
                    style={{ flex: 1, justifyContent: 'flex-start' }}
                    data-testid={`mapping-option-${currentLanguage}_embedded`}
                  >
                    {isEmbeddedSelected && <CheckIcon />}
                    {currentLanguage}_embedded
                    <Text size="1" color="gray" style={{ marginLeft: 'auto' }}>
                      {isCustomizedEmbedded ? '(Customized)' : '(Embedded)'}
                    </Text>
                  </Button>
                </Flex>
              )}

              {/* Custom Mapping Options */}
              {customMappings.map((mapping, index) => (
                <Flex key={index} align="center" gap="2" mb="1">
                  <Button
                    variant={selectedMapping?.name === mapping.name ? "solid" : "outline"}
                    size="1"
                    onClick={() => selectCustomMapping(mapping)}
                    style={{ flex: 1, justifyContent: 'flex-start' }}
                    data-testid={`mapping-option-${mapping.name}`}
                  >
                    {selectedMapping?.name === mapping.name && <CheckIcon />}
                    {mapping.name}
                    <Text size="1" color="gray" style={{ marginLeft: 'auto' }}>(Custom)</Text>
                  </Button>
                  <Button
                    variant="ghost"
                    size="1"
                    onClick={() => removeCustomMapping(index)}
                    color="red"
                  >
                    <TrashIcon />
                  </Button>
                </Flex>
              ))}

              {/* Clear Selection */}
              {selectedMapping && (
                <Flex align="center" gap="2" mt="2">
                  <Button
                    variant="ghost"
                    size="1"
                    onClick={clearSelectedMapping}
                    color="gray"
                  >
                    Clear Selection
                  </Button>
                </Flex>
              )}
            </Box>

            {/* Selected Mapping Editor */}
            {selectedMapping && (
              <Flex direction="column" style={{ flex: 1 }} gap="2">
                <Flex justify="between" align="center">
                  <Flex align="center" gap="2">
                    <TextField.Root
                      value={selectedMapping.name}
                      disabled={selectedMapping.isEmbedded}
                      onChange={(e) => {
                        if (selectedMapping.isEmbedded) {
                          updateSelectedMapping('name', e.target.value)
                        } else {
                          const index = customMappings.findIndex(m => m === selectedMapping)
                          if (index !== -1) {
                            updateCustomMapping(index, 'name', e.target.value)
                          }
                        }
                      }}
                      size="1"
                      style={{ width: '120px' }}
                    />
                    <TextField.Root
                      value={selectedMapping.extensions?.join(', ') || ''}
                      disabled={selectedMapping.isEmbedded}
                      onChange={(e) => {
                        if (selectedMapping.isEmbedded) {
                          updateSelectedMapping('extensions', e.target.value.split(',').map(ext => ext.trim()))
                        } else {
                          const index = customMappings.findIndex(m => m === selectedMapping)
                          if (index !== -1) {
                            updateCustomMapping(index, 'extensions', e.target.value.split(',').map(ext => ext.trim()))
                          }
                        }
                      }}
                      size="1"
                      style={{ width: '150px' }}
                    />
                  </Flex>
                  <Flex align="center" gap="2">
                    {isCustomizedEmbedded && (
                      <Button
                        variant="ghost"
                        size="1"
                        onClick={resetEmbeddedMapping}
                        color="orange"
                        title="Reset to original embedded mapping"
                      >
                        <ResetIcon />
                        Reset
                      </Button>
                    )}
                    <Text size="1" color="gray">
                      {isCustomizedEmbedded ? 'Customized Embedded Mapping' : 
                       selectedMapping.isEmbedded ? 'Embedded Mapping' : 'Custom Mapping'}
                    </Text>
                  </Flex>
                </Flex>
                
                <Box style={{ flex: 1, minHeight: 0 }}>
                  <textarea
                    value={selectedMapping.uast || ''}
                    onChange={(e) => {
                      if (selectedMapping.isEmbedded) {
                        updateSelectedMapping('uast', e.target.value)
                      } else {
                        const index = customMappings.findIndex(m => m === selectedMapping)
                        if (index !== -1) {
                          updateCustomMapping(index, 'uast', e.target.value)
                        }
                      }
                    }}
                    placeholder={selectedMapping.isEmbedded ? "Edit embedded mapping content..." : "Enter UAST mapping DSL..."}
                    style={{
                      width: '100%',
                      height: '100%',
                      padding: '8px',
                      fontSize: '12px',
                      fontFamily: 'monospace',
                      backgroundColor: 'var(--color-surface)',
                      border: '1px solid var(--gray-6)',
                      borderRadius: '4px',
                      resize: 'none',
                      outline: 'none',
                      boxSizing: 'border-box'
                    }}
                    data-testid="dsl-content"
                  />
                </Box>
              </Flex>
            )}
          </Flex>
        )}
      </Box>
    </Box>
  )
} 