import { useEffect, useRef } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { useProjectExplorer } from '../../hooks/useProjectExplorer';
import type { ProjectFolder } from '../../types/project-explorer';
import { ProjectList } from './ProjectList';
import './ProjectExplorer.css';

interface ProjectExplorerProps {
  onSelectFolder?: (folder: ProjectFolder) => void;
}

export function ProjectExplorer({ onSelectFolder }: ProjectExplorerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const {
    projects,
    focusedFolderId,
    showHotkeys,
    toggleProject,
    selectFolder,
    openPullRequest,
    handleKeyDown,
    handleKeyUp,
    addProject,
    addFolder,
  } = useProjectExplorer();

  // Wrap selectFolder to also call onSelectFolder
  const handleSelectFolder = (folder: ProjectFolder) => {
    selectFolder(folder);
    onSelectFolder?.(folder);
  };

  const handleAddProject = async () => {
    const selected = await open({ directory: true, multiple: false, title: 'Select Project Folder' });
    if (selected) {
      const name = selected.split('/').pop() || selected;
      addProject(selected, name);
    }
  };

  const handleAddFolder = async (projectId: string) => {
    const selected = await open({ directory: true, multiple: false, title: 'Add Folder to Project' });
    if (selected) {
      const name = selected.split('/').pop() || selected;
      addFolder(projectId, selected, name);
    }
  };

  // Attach keyboard listeners
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    // Use window listeners for global shortcuts (cmd+1-9)
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);

    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
    };
  }, [handleKeyDown, handleKeyUp]);

  const containerClass = [
    'project-explorer',
    showHotkeys ? 'project-explorer--show-hotkeys' : '',
  ].filter(Boolean).join(' ');

  return (
    <div
      ref={containerRef}
      className={containerClass}
      tabIndex={-1}
    >
      <div className="project-explorer-header">
        <h3 className="project-explorer-title">Projects</h3>
        <button
          className="add-project-btn"
          onClick={handleAddProject}
          title="Add project"
        >
          +
        </button>
      </div>

      {projects.length === 0 ? (
        <div className="project-explorer-empty">
          <p>No projects yet</p>
          <button className="add-project-empty-btn" onClick={handleAddProject}>
            Add Project
          </button>
        </div>
      ) : (
        <ProjectList
          projects={projects}
          onToggleProject={toggleProject}
          onSelectFolder={handleSelectFolder}
          onPRClick={openPullRequest}
          onAddFolder={handleAddFolder}
          focusedFolderId={focusedFolderId}
        />
      )}
    </div>
  );
}
