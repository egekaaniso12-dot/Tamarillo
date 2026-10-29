import React from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import DragOverlay from './DragOverlay';
import { getCurrentWindow } from '@tauri-apps/api/window';
import './styles.css';
const Root = getCurrentWindow().label === 'drag-overlay' ? DragOverlay : App;
createRoot(document.getElementById('root')!).render(<React.StrictMode><Root /></React.StrictMode>);
