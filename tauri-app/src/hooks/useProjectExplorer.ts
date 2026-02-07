import { useState, useCallback, useEffect, useMemo } from 'react';
import type { Project, ProjectFolder } from '../types/project-explorer';
import { useLocalStorage } from './useLocalStorage';

function stableId(prefix: string, path: string): string {
  let hash = 0;
  for (let i = 0; i < path.length; i++) {
    hash = ((hash << 5) - hash + path.charCodeAt(i)) | 0;
  }
  return `${prefix}-${(hash >>> 0).toString(36)}`;
}

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
  const [projects, setProjects] = useLocalStorage<Project[]>('libg:projects', []);
  const [activeFolderId, setActiveFolderId] = useLocalStorage<string | null>('libg:activeFolderId', null);
  const [focusedFolderId, setFocusedFolderId] = useState<string | null>(null);
  const [showHotkeys, setShowHotkeys] = useState(false);

  const activeFolder = useMemo(() => {
    if (!activeFolderId) return null;
    for (const p of projects) {
      const f = p.folders.find((f) => f.id === activeFolderId);
      if (f) return f;
    }
    return null;
  }, [activeFolderId, projects]);

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
  }, [setProjects]);

  const selectFolder = useCallback((folder: ProjectFolder) => {
    setActiveFolderId(folder.id);
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
  }, [setActiveFolderId, setProjects]);

  const openPullRequest = useCallback((pr: { url?: string; number: number }) => {
    if (pr.url) {
      window.open(pr.url, '_blank');
    }
  }, []);

  const addProject = useCallback((path: string, name: string) => {
    const projectId = stableId('proj', path);
    const folderId = stableId('folder', path);
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
    setProjects((prev) => {
      // Don't add duplicate projects
      if (prev.some((p) => p.id === projectId)) return prev;
      return [...prev, project];
    });
  }, [setProjects]);

  const addFolder = useCallback((projectId: string, path: string, name: string) => {
    const folderId = stableId('folder', path);
    const folder: ProjectFolder = {
      id: folderId,
      name,
      path,
      branch: '',
      diffStats: null,
      pullRequest: null,
    };
    setProjects((prev) =>
      prev.map((p) => {
        if (p.id !== projectId) return p;
        // Don't add duplicate folders
        if (p.folders.some((f) => f.id === folderId)) return p;
        return { ...p, folders: [...p.folders, folder] };
      })
    );
  }, [setProjects]);

  const removeProject = useCallback((projectId: string) => {
    setProjects((prev) => {
      const remaining = prev.filter((p) => p.id !== projectId);
      // Clear active folder if it belonged to the removed project
      const removed = prev.find((p) => p.id === projectId);
      if (removed && activeFolderId) {
        const wasInRemoved = removed.folders.some((f) => f.id === activeFolderId);
        if (wasInRemoved) {
          setActiveFolderId(null);
        }
      }
      return remaining;
    });
  }, [activeFolderId, setActiveFolderId, setProjects]);

  const removeFolder = useCallback((folderId: string) => {
    setProjects((prev) =>
      prev.map((p) => ({
        ...p,
        folders: p.folders.filter((f) => f.id !== folderId),
      }))
    );
    if (activeFolderId === folderId) {
      setActiveFolderId(null);
    }
  }, [activeFolderId, setActiveFolderId, setProjects]);

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
  }, [allFolders, projects, getAllFolders, focusedFolderId, selectFolder, setProjects]);

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
