import React, { useState, useEffect } from 'react'
import { Button, IconButton } from '@radix-ui/themes'
import { SunIcon, MoonIcon } from '@radix-ui/react-icons'

export function ThemeToggle() {
  const [theme, setTheme] = useState(() => {
    const savedTheme = localStorage.getItem('theme')
    return savedTheme || 'light'
  })

  useEffect(() => {
    // Update Radix UI theme
    const themeElement = document.querySelector('[data-radix-theme]')
    if (themeElement) {
      themeElement.setAttribute('data-radix-theme-appearance', theme)
    }
    
    // Save theme preference
    localStorage.setItem('theme', theme)
  }, [theme])

  const toggleTheme = () => {
    setTheme(prevTheme => prevTheme === 'light' ? 'dark' : 'light')
  }

  return (
    <IconButton
      variant="ghost"
      size="2"
      onClick={toggleTheme}
      title={`Switch to ${theme === 'light' ? 'dark' : 'light'} theme`}
    >
               {theme === 'light' ? <MoonIcon /> : <SunIcon />}
    </IconButton>
  )
} 