import { Placeholder } from "../../components/ui/Placeholder";
import { LogViewer } from "./LogViewer";

export function SettingsPage() {
  return (
    <div className="flex h-full min-h-0 flex-col gap-4">
      <Placeholder title="Configuració" phase={3}>
        Interval de congelació, hotkeys personalitzades i jocs afegits a mà.
      </Placeholder>
      <LogViewer />
    </div>
  );
}
