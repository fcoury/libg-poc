import { useState, useCallback, useEffect, useMemo } from 'react';
import type { Project, ProjectFolder } from '../types/project-explorer';

let nextProjectId = 1;
let nextFolderId = 1;

export interface UseProjectExplorerReturn {
  projects: Project[];
  activeFolder: ProjectFolder | null;
  focusedFolderId: string | null;
  showHotkeys: boolean;
  toggleProject: (projectId: string) => void;
  selectFolder: (folder: ProjectFolder) => void;
  openPullRequest: (pr: { url?: string; number: number }) => void;
  handleKeyDown: (e: KeyboardEvent) => void;
  handleKeyUp: (e: KeyboardEvent) => void;
  getAllFolders: () => ProjectFolder[];
  addProject: (path: string, name: string) => void;
  addFolder: (projectId: string, path: string, name: string) => void;
  removeProject: (projectId: string) => void;
  removeFolder: (folderId: string) => void;
}

export function useProjectExplorer(): UseProjectExplorerReturn {
  const [projects, setProjects] = useState<Project[]>([]);
  const [activeFolder, setActiveFolder] = useState<ProjectFolder | null>(null);
  const [focusedFolderId, setFocusedFolderId] = useState<string | null>(null);
  const [showHotkeys, setShowHotkeys] = useState(false);

  // Get all folders from expanded projects (for keyboard navigation)
  const getAllFolders = useCallback((): ProjectFolder[] => {
    return projects
      .filter((p) => p.isExpanded)
      .flatMap((p) => p.folders);
  }, [projects]);

  // Get all folders regardless of expansion state (for cmd+1-9 shortcuts)
  const allFolders = useMemo(() => {
    return projects.flatMap((p) => p.folders);
  }, [projects]);

  const toggleProject = useCallback((projectId: string) => {
    setProjects((prev) =>
      prev.map((p) =>
        p.id === projectId ? { ...p, isExpanded: !p.isExpanded } : p
      )
    );
  }, []);

  const selectFolder = useCallback((folder: ProjectFolder) => {
    setActiveFolder(folder);
    setFocusedFolderId(folder.id);
    // Update active state across all projects
    setProjects((prev) =>
      prev.map((p) => ({
        ...p,
        folders: p.folders.map((f) => ({
          ...f,
          isActive: f.id === folder.id,
        })),
      }))
    );
  }, []);

  const openPullRequest = useCallback((pr: { url?: string; number: number }) => {
    if (pr.url) {
      window.open(pr.url, '_blank');
    }
  }, []);

  const addProject = useCallback((path: string, name: string) => {
    const projectId = `proj-${nextProjectId++}`;
    const folderId = `folder-${nextFolderId++}`;
    const folder: ProjectFolder = {
      id: folderId,
      name,
      path,
      branch: '',
      diffStats: null,
      pullRequest: null,
      isActive: false,
    };
    const project: Project = {
      id: projectId,
      name,
      rootPath: path,
      folders: [folder],
      isExpanded: true,
    };
    setProjects((prev) => [...prev, project]);
  }, []);

  const addFolder = useCallback((projectId: string, path: string, name: string) => {
    const folderId = `folder-${nextFolderId++}`;
    const folder: ProjectFolder = {
      id: folderId,
      name,
      path,
      branch: '',
      diffStats: null,
      pullRequest: null,
    };
    setProjects((prev) =>
      prev.map((p) =>
        p.id === projectId
          ? { ...p, folders: [...p.folders, folder] }
          : p
      )
    );
  }, []);

  const removeProject = useCallback((projectId: string) => {
    setProjects((prev) => prev.filter((p) => p.id !== projectId));
    setActiveFolder((prev) => {
      if (!prev) return null;
      // Clear active folder if it belonged to the removed project
      const stillExists = projects.some(
        (p) => p.id !== projectId && p.folders.some((f) => f.id === prev.id)
      );
      return stillExists ? prev : null;
    });
  }, [projects]);

  const removeFolder = useCallback((folderId: string) => {
    setProjects((prev) =>
      prev.map((p) => ({
        ...p,
        folders: p.folders.filter((f) => f.id !== folderId),
      }))
    );
    setActiveFolder((prev) => prev?.id === folderId ? null : prev);
  }, []);

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    // Show hotkeys when meta key is held
    if (e.key === 'Meta') {
      setShowHotkeys(true);
    }

    // cmd+1-9 for quick folder access
    if (e.metaKey && e.key >= '1' && e.key <= '9') {
      e.preventDefault();
      const index = parseInt(e.key) - 1;
      if (allFolders[index]) {
        // Ensure the project containing this folder is expanded
        const targetFolder = allFolders[index];
        const containingProject = projects.find((p) =>
          p.folders.some((f) => f.id === targetFolder.id)
        );
        if (containingProject && !containingProject.isExpanded) {
          setProjects((prev) =>
            prev.map((p) =>
              p.id === containingProject.id ? { ...p, isExpanded: true } : p
            )
          );
        }
        selectFolder(targetFolder);
      }
      return;
    }

    // Arrow key navigation within expanded folders
    const visibleFolders = getAllFolders();
    if (visibleFolders.length === 0) return;

    const currentIndex = focusedFolderId
      ? visibleFolders.findIndex((f) => f.id === focusedFolderId)
      : -1;

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      const nextIndex = currentIndex < visibleFolders.length - 1 ? currentIndex + 1 : 0;
      setFocusedFolderId(visibleFolders[nextIndex].id);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      const prevIndex = currentIndex > 0 ? currentIndex - 1 : visibleFolders.length - 1;
      setFocusedFolderId(visibleFolders[prevIndex].id);
    } else if ((e.key === 'Enter' || e.key === ' ') && focusedFolderId) {
      e.preventDefault();
      const folder = visibleFolders.find((f) => f.id === focusedFolderId);
      if (folder) {
        selectFolder(folder);
      }
    }
  }, [allFolders, projects, getAllFolders, focusedFolderId, selectFolder]);

  const handleKeyUp = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Meta') {
      setShowHotkeys(false);
    }
  }, []);

  // Set initial focus to active folder
  useEffect(() => {
    if (activeFolder && !focusedFolderId) {
      setFocusedFolderId(activeFolder.id);
    }
  }, [activeFolder, focusedFolderId]);

  return {
    projects,
    activeFolder,
    focusedFolderId,
    showHotkeys,
    toggleProject,
    selectFolder,
    openPullRequest,
    handleKeyDown,
    handleKeyUp,
    getAllFolders,
    addProject,
    addFolder,
    removeProject,
    removeFolder,
  };
}
