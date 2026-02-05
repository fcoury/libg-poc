import { BranchIcon } from './icons';

interface BranchBadgeProps {
  branch: string;
}

export function BranchBadge({ branch }: BranchBadgeProps) {
  // Truncate long branch names at 16 chars
  const displayBranch = branch.length > 16
    ? `${branch.slice(0, 13)}...`
    : branch;

  return (
    <span className="branch-badge" title={branch}>
      <BranchIcon size={10} />
      <span className="branch-name">{displayBranch}</span>
    </span>
  );
}
