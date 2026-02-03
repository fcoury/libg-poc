import { Ghostty } from "./components/Ghostty";
import "./App.css";

function App() {
  return (
    <main className="app-shell">
      <header className="toolbar">
        <div className="toolbar-title">Ghostty + Tauri</div>
        <div className="toolbar-actions">
          <button type="button">Split</button>
          <button type="button">Settings</button>
        </div>
      </header>

      <section className="content">
        <div className="side-panel">
          <h3>Chrome</h3>
          <p>Use this panel to build your UI around the terminal.</p>
        </div>
        <div className="terminal-panel">
          <Ghostty id="main-terminal" className="ghostty-host" />
        </div>
      </section>
    </main>
  );
}

export default App;
