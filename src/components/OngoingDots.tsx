/**
 * Shared ongoing/active indicator — renders 1 or more pulsing green dots.
 * Use `count={1}` for compact spots (tree, picker) and `count={5}` for detail views.
 */
interface OngoingDotsProps {
  /** Number of dots to render (default: 5) */
  count?: number;
  className?: string;
}

export function OngoingDots({ count = 5, className }: OngoingDotsProps) {
  const dots = [];
  for (let i = 0; i < count; i++) {
    dots.push(<span key={i} className="ongoing-dot" />);
  }
  return <span className={`ongoing-dots${className ? ` ${className}` : ""}`}>{dots}</span>;
}
