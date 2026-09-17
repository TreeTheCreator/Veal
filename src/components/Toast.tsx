import { useUi } from "../ui";

export function Toast() {
  const toast = useUi((s) => s.toast);
  return (
    <div className={`toast${toast ? " show" : ""}`} role="status">
      {toast}
    </div>
  );
}
