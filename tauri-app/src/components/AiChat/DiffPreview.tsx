import type { BufferEdit } from "../../types/nvim";

type Props = {
  edit: BufferEdit;
};

export function DiffPreview({ edit }: Props) {
  return (
    <div className="diff-preview">
      <div className="diff-preview__header">
        Lines {edit.startLine + 1}&ndash;{edit.endLine}
      </div>
      <div className="diff-preview__content">
        {edit.newLines.map((line, i) => (
          <div key={i} className="diff-preview__line diff-preview__line--added">
            <span className="diff-preview__line-num">{edit.startLine + i + 1}</span>
            <span className="diff-preview__line-text">{line || " "}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
