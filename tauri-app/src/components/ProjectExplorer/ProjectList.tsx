import type { Project, ProjectFolder, PullRequest } from '../../types/project-explorer';
import { ProjectItem } from './ProjectItem';

interface ProjectListProps {
  projects: Project[];
  onToggleProject: (projectId: string) => void;
  onSelectFolder: (folder: ProjectFolder) => void;
  onPRClick: (pr: PullRequest) => void;
  onAddFolder: (projectId: string) => void;
  focusedFolderId?: string | null;
}

export function ProjectList({
  projects,
  onToggleProject,
  onSelectFolder,
  onPRClick,
  onAddFolder,
  focusedFolderId,
}: ProjectListProps) {
  // Calculate global folder start index for each project
  // This is used for cmd+1-9 hotkey badges
  let globalFolderIndex = 0;

  return (
    <div className="project-list" role="tree" aria-label="Projects">
      {projects.map((project) => {
        const startIndex = globalFolderIndex;
        globalFolderIndex += project.folders.length;

        return (
          <ProjectItem
            key={project.id}
            project={project}
            onToggle={onToggleProject}
            onSelectFolder={onSelectFolder}
            onPRClick={onPRClick}
            onAddFolder={onAddFolder}
            globalFolderStartIndex={startIndex}
            focusedFolderId={focusedFolderId}
          />
        );
      })}
    </div>
  );
}
