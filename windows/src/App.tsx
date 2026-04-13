import { FloatingBar } from './components/FloatingBar/FloatingBar';
import { SettingsWindow } from './components/Settings/SettingsWindow';

function App() {
  const pathname = window.location.pathname;

  if (pathname === '/floating-bar') {
    return <FloatingBar />;
  }

  return <SettingsWindow />;
}

export default App;
