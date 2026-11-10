import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { App } from './App'
import './tokens.css'
import './app.css'

const element = document.getElementById('root')
if (element === null) throw new Error('The shell has no root element.')

createRoot(element).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
