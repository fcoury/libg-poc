import type { Project, ProjectFolder, PullRequest } from '../../types/project-explorer';
import { ChevronIcon, ProjectIcon } from './icons';
import { FolderItem } from './FolderItem';

interface ProjectItemProps {
  project: Project;
  onToggle: (projectId: string) => void;
  onSelectFolder: (folder: ProjectFolder) => void;
  onPRClick: (pr: PullRequest) => void;
  onAddFolder: (projectId: string) => void;
  globalFolderStartIndex: number;
  focusedFolderId?: string | null;
}

export function ProjectItem({
  project,
  onToggle,
  onSelectFolder,
  onPRClick,
  onAddFolder,
  globalFolderStartIndex,
  focusedFolderId,
}: ProjectItemProps) {
  const handleHeaderClick = () => {
    onToggle(project.id);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onToggle(project.id);
    } else if (e.key === 'ArrowRight' && !project.isExpanded) {
      e.preventDefault();
      onToggle(project.id);
    } else if (e.key === 'ArrowLeft' && project.isExpanded) {
      e.preventDefault();
      onToggle(project.id);
    }
  };

  const handleAddFolderClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    onAddFolder(project.id);
  };

  return (
    <div className="project-item" role="treeitem" aria-expanded={project.isExpanded}>
      <div
        className="project-header"
        onClick={handleHeaderClick}
        onKeyDown={handleKeyDown}
        role="button"
        tabIndex={0}
        aria-label={`${project.name}, ${project.folders.length} folders`}
      >
        <ChevronIcon
          className={`project-chevron ${project.isExpanded ? 'project-chevron--expanded' : ''}`}
          size={14}
        />
        <ProjectIcon className="project-icon" size={14} />
        <span className="project-name">{project.name}</span>
        <button
          className="add-folder-btn"
          onClick={handleAddFolderClick}
          title="Add folder"
        >
          +
        </button>
        <span className="project-count">{project.folders.length}</span>
      </div>
      {project.isExpanded && (
        <div className="project-folders" role="group">
          {project.folders.map((folder, index) => (
            <FolderItem
              key={folder.id}
              folder={folder}
              onSelect={onSelectFolder}
              onPRClick={onPRClick}
              globalIndex={globalFolderStartIndex + index}
              isFocused={focusedFolderId === folder.id}
            />
          ))}
        </div>
      )}
    </div>
  );
}
