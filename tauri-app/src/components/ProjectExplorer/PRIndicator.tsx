import type { PullRequest } from '../../types/project-explorer';
import { PRIcon, CheckIcon } from './icons';

interface PRIndicatorProps {
  pr: PullRequest;
  onClick?: () => void;
}

export function PRIndicator({ pr, onClick }: PRIndicatorProps) {
  const stateClass = `pr-indicator pr-${pr.state}`;

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    onClick?.();
  };

  return (
    <button
      type="button"
      className={stateClass}
      onClick={handleClick}
      title={`PR #${pr.number} (${pr.state})`}
    >
      <PRIcon size={12} />
      <span>#{pr.number}</span>
      {pr.state === 'merged' && (
        <span className="pr-check-overlay">
          <CheckIcon size={6} />
        </span>
      )}
    </button>
  );
}
