import type { GitDiffStats } from '../../types/project-explorer';

interface DiffStatsProps {
  stats: GitDiffStats;
}

export function DiffStats({ stats }: DiffStatsProps) {
  return (
    <span className="diff-stats-text">
      <span className="diff-additions">+{stats.additions}</span>
      {' '}
      <span className="diff-deletions">-{stats.deletions}</span>
    </span>
  );
}
