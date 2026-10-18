import React from 'react';
import { ShellLayout } from './shell/ShellLayout';
import { ToastProvider } from './components/feedback';

export const App: React.FC = () => {
  return (
    <ToastProvider>
      <ShellLayout />
    </ToastProvider>
  );
};

export default App;
