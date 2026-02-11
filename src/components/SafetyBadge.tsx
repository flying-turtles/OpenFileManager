interface Props {
  totalCopies: number;
  coldCopies: number;
  isSafe: boolean;
}

export function SafetyBadge({ totalCopies, coldCopies, isSafe }: Props) {
  if (isSafe) {
    return (
      <span className="badge badge-safe" title={`${totalCopies} copies, ${coldCopies} cold`}>
        Safe
      </span>
    );
  }
  if (totalCopies >= 2) {
    return (
      <span className="badge badge-warn" title="No cold backup">
        No Cold
      </span>
    );
  }
  return (
    <span className="badge badge-danger" title={`Only ${totalCopies} copy`}>
      Unsafe
    </span>
  );
}
