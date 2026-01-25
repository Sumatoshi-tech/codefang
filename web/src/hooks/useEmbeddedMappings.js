import { useState, useEffect } from 'react'

export function useEmbeddedMappings() {
  const [embeddedMappingsList, setEmbeddedMappingsList] = useState({})
  const [loadedMappings, setLoadedMappings] = useState({})
  const [customizedEmbeddedMappings, setCustomizedEmbeddedMappings] = useState({})
  const [isLoading, setIsLoading] = useState(true)
  const [error, setError] = useState(null)

  useEffect(() => {
    fetchEmbeddedMappingsList()
  }, [])

  const fetchEmbeddedMappingsList = async () => {
    try {
      setIsLoading(true)
      setError(null)
      
      const response = await fetch('/api/mappings')
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`)
      }
      
      const mappings = await response.json()
      setEmbeddedMappingsList(mappings)
    } catch (err) {
      console.error('Error fetching embedded mappings list:', err)
      setError(err.message)
      // Set empty object to prevent hanging
      setEmbeddedMappingsList({})
    } finally {
      setIsLoading(false)
    }
  }

  const loadMappingForLanguage = async (language) => {
    // If already loaded, return it
    if (loadedMappings[language]) {
      return loadedMappings[language]
    }

    // If language doesn't exist in the list, return null
    if (!embeddedMappingsList[language]) {
      return null
    }

    try {
      const response = await fetch(`/api/mappings/${language}`)
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`)
      }
      
      const mapping = await response.json()
      
      // Store the loaded mapping
      setLoadedMappings(prev => ({
        ...prev,
        [language]: mapping
      }))
      
      return mapping
    } catch (err) {
      console.error(`Error loading mapping for ${language}:`, err)
      throw err
    }
  }

  const getMappingForLanguage = (language) => {
    return loadedMappings[language] || null
  }

  const getAvailableLanguages = () => {
    return Object.keys(embeddedMappingsList)
  }

  const getFormattedLanguages = () => {
    return Object.keys(embeddedMappingsList).map(lang => {
      // Handle special cases
      const specialCases = {
        'c_sharp': 'C#',
        'cpp': 'C++',
        'c': 'C',
        'tsx': 'TSX',
        'rust_with_rstml': 'Rust (RSTML)',
        'markdown_inline': 'Markdown Inline',
        'nim_format_string': 'Nim Format String',
        'git_config': 'Git Config',
        'ssh_config': 'SSH Config',
        'gosum': 'Go Sum',
        'gotmpl': 'Go Template',
        'gowork': 'Go Work'
      }
      
      const label = specialCases[lang] || lang.charAt(0).toUpperCase() + lang.slice(1)
      
      return {
        value: lang,
        label: label
      }
    }).sort((a, b) => a.label.localeCompare(b.label)) // Sort alphabetically
  }

  // New functions for handling customized embedded mappings
  const getCustomizedEmbeddedMapping = (language) => {
    return customizedEmbeddedMappings[language] || null
  }

  const updateCustomizedEmbeddedMapping = (language, mapping) => {
    setCustomizedEmbeddedMappings(prev => ({
      ...prev,
      [language]: mapping
    }))
  }

  const resetCustomizedEmbeddedMapping = (language) => {
    setCustomizedEmbeddedMappings(prev => {
      const newState = { ...prev }
      delete newState[language]
      return newState
    })
  }

  const hasCustomizedEmbeddedMapping = (language) => {
    return customizedEmbeddedMappings[language] !== undefined
  }

  return {
    embeddedMappingsList,
    loadedMappings,
    customizedEmbeddedMappings,
    isLoading,
    error,
    getMappingForLanguage,
    getAvailableLanguages,
    getFormattedLanguages,
    loadMappingForLanguage,
    getCustomizedEmbeddedMapping,
    updateCustomizedEmbeddedMapping,
    resetCustomizedEmbeddedMapping,
    hasCustomizedEmbeddedMapping,
    refetch: fetchEmbeddedMappingsList
  }
} 