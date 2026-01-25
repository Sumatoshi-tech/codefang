import { useState } from 'react'

export function useCustomMappings() {
  const [customMappings, setCustomMappings] = useState([])

  const addCustomMapping = () => {
    const newMapping = {
      name: `custom_mapping_${customMappings.length + 1}`,
      extensions: ['.ext'],
      uast: `[language "custom", extensions: ".ext"]

// Add your custom UAST mapping rules here
// Example:
// identifier <- (identifier) => uast(
//     type: "CustomIdentifier"
// )`
    }
    setCustomMappings([...customMappings, newMapping])
    return newMapping
  }

  const removeCustomMapping = (index) => {
    const newMappings = customMappings.filter((_, i) => i !== index)
    setCustomMappings(newMappings)
  }

  const updateCustomMapping = (index, field, value) => {
    const newMappings = [...customMappings]
    newMappings[index] = { ...newMappings[index], [field]: value }
    setCustomMappings(newMappings)
  }

  const getCustomMapping = (index) => {
    return customMappings[index] || null
  }

  const getAllCustomMappings = () => {
    return customMappings
  }

  return {
    customMappings,
    setCustomMappings,
    addCustomMapping,
    removeCustomMapping,
    updateCustomMapping,
    getCustomMapping,
    getAllCustomMappings
  }
} 