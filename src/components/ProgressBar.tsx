interface Props {
  scanned: number;
  total: number;
  hashed: number;
  lastFile: string;
}

export function ProgressBar({ scanned, total, hashed, lastFile }: Props) {
  const pct = total > 0 ? (scanned / total) * 100 : 0;

  return (
    <div className="progress-container">
      <div className="progress-bar">
        <div className="progress-fill" style={{ width: `${pct}%` }} />
      </div>
      <div className="progress-stats">
        <span>
          {scanned} / {total} files scanned
        </span>
        <span>{hashed} hashed</span>
      </div>
      {lastFile && <div className="progress-file">{lastFile}</div>}
    </div>
  );
}
