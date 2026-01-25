import React from 'react'
import ReactDOM from 'react-dom/client'
import { Theme } from '@radix-ui/themes'
import '@radix-ui/themes/styles.css'
import App from './App.jsx'

// Initialize theme from localStorage
const savedTheme = localStorage.getItem('theme') || 'light'

ReactDOM.createRoot(document.getElementById('root')).render(
  <React.StrictMode>
    <Theme 
      accentColor="brown" 
      radius="small" 
      scaling="95%"
      appearance={savedTheme}
    >
      <App />
    </Theme>
  </React.StrictMode>,
) 