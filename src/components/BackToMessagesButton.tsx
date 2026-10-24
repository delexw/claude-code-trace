import { BackIcon } from "./Icons";

export function BackToMessagesButton({ onClick }: { onClick: () => void }) {
  return (
    <button className="view-toolbar__btn" onClick={onClick}>
      <BackIcon /> Back to Messages
    </button>
  );
}
