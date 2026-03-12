interface Props {
  scanned: number;
  total: number;
  toHash: number;
  skipped: number;
  hashed: number;
  lastFile: string;
}

export function ProgressBar({ scanned, total, toHash, skipped, hashed, lastFile }: Props) {
  const scanPct = total > 0 ? (scanned / total) * 100 : 0;
  const hashPct = toHash > 0 ? (hashed / toHash) * 100 : 0;

  return (
    <div className="progress-container">
      <div className="progress-bar">
        <div className="progress-fill" style={{ width: `${scanPct}%` }} />
      </div>
      <div className="progress-stats">
        <span>
          {scanned} / {total} files scanned
        </span>
        {skipped > 0 && <span>{skipped} skipped</span>}
      </div>
      {toHash > 0 && (
        <>
          <div className="progress-bar">
            <div className="progress-fill hashing" style={{ width: `${hashPct}%` }} />
          </div>
          <div className="progress-stats">
            <span>
              {hashed} / {toHash} files hashed
            </span>
          </div>
        </>
      )}
      {lastFile && <div className="progress-file">{lastFile}</div>}
    </div>
  );
}
