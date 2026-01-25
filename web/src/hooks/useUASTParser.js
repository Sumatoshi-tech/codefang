import { useState, useEffect, useCallback, useRef } from 'react'

export function useUASTParser(loadMappingForLanguage, embeddedMappingsHook) {
  const [code, setCode] = useState('')
  const [queryInput, setQueryInput] = useState('')
  const [uastOutput, setUastOutput] = useState('No UAST data yet. Start typing code to parse automatically.')
  const [queryOutput, setQueryOutput] = useState('No query results yet. Enter a query to see results.')
  const [isParsing, setIsParsing] = useState(false)
  const [isQuerying, setIsQuerying] = useState(false)
  const [currentLanguage, setCurrentLanguage] = useState('go')
  const [selectedMapping, setSelectedMapping] = useState(null)
  const [currentEmbeddedMapping, setCurrentEmbeddedMapping] = useState(null)
  
  // Use refs to prevent infinite loops
  const isInitialized = useRef(false)
  const lastLanguage = useRef(currentLanguage)

  // Load embedded mapping when language changes
  useEffect(() => {
    // Only trigger if language actually changed
    if (lastLanguage.current === currentLanguage) {
      return
    }
    
    console.log('useEffect triggered for language change:', currentLanguage)
    lastLanguage.current = currentLanguage
    
    const loadEmbeddedMapping = async () => {
      if (!loadMappingForLanguage) {
        console.log('loadMappingForLanguage not available yet')
        return
      }
      
      console.log(`Loading embedded mapping for language: ${currentLanguage}`)
      
      try {
        const mapping = await loadMappingForLanguage(currentLanguage)
        if (mapping) {
          console.log(`Successfully loaded embedded mapping for ${currentLanguage}:`, mapping.extensions)
          setCurrentEmbeddedMapping(mapping)
          
          // Check if there's a customized version of this embedded mapping
          const customizedMapping = embeddedMappingsHook?.getCustomizedEmbeddedMapping(currentLanguage)
          
          if (customizedMapping) {
            // Use the customized version
            console.log('Using customized embedded mapping for', currentLanguage)
            setSelectedMapping({
              ...customizedMapping,
              isEmbedded: true,
              isCustomized: true
            })
          } else {
            // Auto-select the original embedded mapping for the current language
            const selectedMappingData = {
              name: `${currentLanguage}_embedded`,
              extensions: mapping.extensions || [`.${currentLanguage}`],
              uast: mapping.uast || '',
              isEmbedded: true,
              isCustomized: false
            }
            console.log('Setting selected mapping:', selectedMappingData)
            setSelectedMapping(selectedMappingData)
          }
        } else {
          console.log(`No embedded mapping available for ${currentLanguage}`)
          setCurrentEmbeddedMapping(null)
          setSelectedMapping(null)
        }
      } catch (error) {
        console.error(`Failed to load embedded mapping for ${currentLanguage}:`, error)
        setCurrentEmbeddedMapping(null)
        setSelectedMapping(null)
      }
    }

    loadEmbeddedMapping()
  }, [currentLanguage, loadMappingForLanguage, embeddedMappingsHook])

  // Main processing effect - follows the specified logic flow
  useEffect(() => {
    // If code is not filled or mapping not selected - Exit
    if (!code.trim() || !selectedMapping) {
      console.log('Skipping processing: code or mapping not available')
      return
    }

    console.log('Processing: code and mapping available, parsing...')
    
    // Parse code with mappings
    const parseCode = async () => {
      setIsParsing(true)
      try {
        const requestBody = {
          code: code,
          language: currentLanguage,
          uastMaps: {
            [selectedMapping.name]: {
              extensions: selectedMapping.extensions || [`.${currentLanguage}`],
              uast: selectedMapping.uast || ''
            }
          }
        }

        const response = await fetch('/api/parse', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify(requestBody),
        })

        if (!response.ok) {
          throw new Error(`HTTP error! status: ${response.status}`)
        }

        const result = await response.json()
        
        // Handle the response structure: {"uast": "{\"children\":[...]}"}
        if (result.uast) {
          try {
            // Parse the nested JSON string
            const uastData = JSON.parse(result.uast)
            const uastString = JSON.stringify(uastData, null, 2)
            setUastOutput(uastString)
            
            // If query is filled, pass result UAST to query
            if (queryInput && queryInput.trim()) {
              console.log('Query is filled, executing query with parsed UAST...')
              await executeQueryWithUast(queryInput, uastString)
            } else {
              console.log('No query, outputting UAST results')
            }
          } catch (parseError) {
            console.error('Error parsing UAST JSON:', parseError)
            setUastOutput(`Error parsing UAST: ${parseError.message}`)
          }
        } else if (result.error) {
          setUastOutput(`Parse error: ${result.error}`)
        } else {
          setUastOutput('No UAST data received from server')
        }
      } catch (error) {
        console.error('Error parsing code:', error)
        setUastOutput(`Error parsing code: ${error.message}`)
      } finally {
        setIsParsing(false)
      }
    }

    parseCode()
  }, [code, selectedMapping, currentLanguage, queryInput])

  // Separate function to execute query with given UAST
  const executeQueryWithUast = async (query, uastData) => {
    if (!query.trim()) {
      setQueryOutput('No query results yet. Enter a query to see results.')
      return
    }

    setIsQuerying(true)
    try {
      const requestBody = {
        uast: uastData,
        query: query
      }

      const response = await fetch('/api/query', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(requestBody),
      })

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`)
      }

      const result = await response.json()
      
      // Handle the response structure: {"results": "{\"nodes\":[...]}"}
      if (result.error) {
        // Handle query errors
        setQueryOutput(`Query Error: ${result.error}`)
      } else if (result.results) {
        try {
          // Parse the nested JSON string
          const queryData = JSON.parse(result.results)
          setQueryOutput(JSON.stringify(queryData, null, 2))
        } catch (parseError) {
          console.error('Error parsing query results JSON:', parseError)
          setQueryOutput(`Error parsing query results: ${parseError.message}`)
        }
      } else {
        setQueryOutput(JSON.stringify(result, null, 2))
      }
    } catch (error) {
      console.error('Error executing query:', error)
      setQueryOutput(`Error executing query: ${error.message}`)
    } finally {
      setIsQuerying(false)
    }
  }

  // Legacy executeQuery function for manual query execution
  const executeQuery = async (query) => {
    if (!uastOutput || uastOutput === 'No UAST data yet. Start typing code to parse automatically.' || uastOutput.includes('Error')) {
      setQueryOutput('No UAST data available. Parse some code first.')
      return
    }

    await executeQueryWithUast(query, uastOutput)
  }

  const updateDefaultLanguage = useCallback((availableLanguages) => {
    // Only update if not already initialized and we have a current language
    if (isInitialized.current || !availableLanguages.length) {
      return
    }
    
    // Only update if current language is not in available languages
    if (availableLanguages.includes(currentLanguage)) {
      isInitialized.current = true
      return
    }
    
    // Prefer common languages
    const preferredLanguages = ['go', 'python', 'javascript', 'java', 'cpp', 'rust']
    const preferred = preferredLanguages.find(lang => availableLanguages.includes(lang))
    if (preferred) {
      setCurrentLanguage(preferred)
    } else {
      setCurrentLanguage(availableLanguages[0])
    }
    isInitialized.current = true
  }, [currentLanguage])

  // Force load embedded mapping when language changes, even if already initialized
  useEffect(() => {
    if (currentLanguage && loadMappingForLanguage && !currentEmbeddedMapping) {
      console.log('Force loading embedded mapping for:', currentLanguage)
      const loadEmbeddedMapping = async () => {
        try {
          const mapping = await loadMappingForLanguage(currentLanguage)
          if (mapping) {
            console.log(`Successfully loaded embedded mapping for ${currentLanguage}:`, mapping.extensions)
            setCurrentEmbeddedMapping(mapping)
            
            // Auto-select the embedded mapping
            const selectedMappingData = {
              name: `${currentLanguage}_embedded`,
              extensions: mapping.extensions || [`.${currentLanguage}`],
              uast: mapping.uast || '',
              isEmbedded: true,
              isCustomized: false
            }
            console.log('Auto-selecting embedded mapping:', selectedMappingData)
            setSelectedMapping(selectedMappingData)
          }
        } catch (error) {
          console.error(`Failed to load embedded mapping for ${currentLanguage}:`, error)
        }
      }
      loadEmbeddedMapping()
    }
  }, [currentLanguage, loadMappingForLanguage, currentEmbeddedMapping])

  const selectEmbeddedMapping = () => {
    if (!currentLanguage) {
      console.warn('No language selected. Please select a language first.')
      return
    }
    
    if (currentEmbeddedMapping) {
      // Check if there's a customized version
      const customizedMapping = embeddedMappingsHook?.getCustomizedEmbeddedMapping(currentLanguage)
      
      if (customizedMapping) {
        setSelectedMapping({
          ...customizedMapping,
          isEmbedded: true,
          isCustomized: true
        })
      } else {
        setSelectedMapping({
          name: `${currentLanguage}_embedded`,
          extensions: currentEmbeddedMapping.extensions || [`.${currentLanguage}`],
          uast: currentEmbeddedMapping.uast || '',
          isEmbedded: true,
          isCustomized: false
        })
      }
    }
  }

  const selectCustomMapping = (mapping) => {
    setSelectedMapping({
      ...mapping,
      isEmbedded: false,
      isCustomized: false
    })
  }

  const clearSelectedMapping = () => {
    setSelectedMapping(null)
  }

  const updateSelectedMapping = (field, value) => {
    if (!selectedMapping) return

    const updatedMapping = {
      ...selectedMapping,
      [field]: value
    }

    // If this is an embedded mapping being modified, save it as customized
    if (selectedMapping.isEmbedded && embeddedMappingsHook) {
      embeddedMappingsHook.updateCustomizedEmbeddedMapping(currentLanguage, updatedMapping)
      updatedMapping.isCustomized = true
    }

    setSelectedMapping(updatedMapping)
  }

  const resetEmbeddedMapping = () => {
    if (selectedMapping?.isEmbedded && embeddedMappingsHook) {
      embeddedMappingsHook.resetCustomizedEmbeddedMapping(currentLanguage)
      
      // Reset to original embedded mapping
      if (currentEmbeddedMapping) {
        setSelectedMapping({
          name: `${currentLanguage}_embedded`,
          extensions: currentEmbeddedMapping.extensions || [`.${currentLanguage}`],
          uast: currentEmbeddedMapping.uast || '',
          isEmbedded: true,
          isCustomized: false
        })
      }
    }
  }

  return {
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
  }
} 