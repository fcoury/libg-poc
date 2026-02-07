import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { Ghostty } from "./components/Ghostty";
import { ProjectExplorer } from "./components/ProjectExplorer";
import { AiChat } from "./components/AiChat";
import { useTerminalManager } from "./hooks/useTerminalManager";
import "./App.css";

type SidePanel = "explorer" | "ai";

function App() {
  const [sidebarWidth, setSidebarWidth] = useState(260);
  const [isResizing, setIsResizing] = useState(false);
  const [activePanel, setActivePanel] = useState<SidePanel>("explorer");
  const { activeTerminalId, terminals, switchToFolder } = useTerminalManager();

  // Auto-switch to AI panel when a nvim-action is received
  useEffect(() => {
    const unlisten = listen("nvim-action", () => {
      setActivePanel("ai");
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleResizeStart = (e: React.MouseEvent) => {
    setIsResizing(true);
    e.preventDefault();
  };

  useEffect(() => {
    if (!isResizing) return;

    const handleMouseMove = (e: MouseEvent) => {
      // Account for content padding (16px on left)
      const newWidth = Math.min(Math.max(e.clientX - 16, 200), 500);
      setSidebarWidth(newWidth);
    };

    const handleMouseUp = () => setIsResizing(false);

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isResizing]);

  return (
    <main className="app-shell">
      <header className="toolbar">
        <div className="toolbar-title">Ghostty + Tauri</div>
        <div className="toolbar-actions">
          <button
            type="button"
            className={activePanel === "ai" ? "toolbar-btn--active" : ""}
            onClick={() =>
              setActivePanel((p) => (p === "ai" ? "explorer" : "ai"))
            }
          >
            AI
          </button>
          <button type="button">Split</button>
          <button type="button">Settings</button>
        </div>
      </header>

      <section
        className={`content ${isResizing ? 'content--resizing' : ''}`}
        style={{ gridTemplateColumns: `${sidebarWidth}px 6px 1fr` }}
      >
        <div className="side-panel">
          {activePanel === "explorer" ? (
            <ProjectExplorer onSelectFolder={switchToFolder} />
          ) : (
            <AiChat terminalId={activeTerminalId} />
          )}
        </div>
        <div
          className={`resize-handle ${isResizing ? 'resize-handle--active' : ''}`}
          onMouseDown={handleResizeStart}
        />
        <div className="terminal-panel">
          {Array.from(terminals.entries()).map(([termId, entry]) => (
            <Ghostty
              key={termId}
              id={termId}
              className="ghostty-host"
              visible={termId === activeTerminalId}
              options={{ workingDirectory: entry.path }}
            />
          ))}
          {terminals.size === 0 && (
            <div className="empty-terminal-state">
              <p>Select a folder to open a terminal</p>
            </div>
          )}
        </div>
      </section>
    </main>
  );
}

export default App;
