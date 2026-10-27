import { ProcessList } from "./ProcessList";
import { TrainerList } from "./TrainerList";

export function LibraryPage() {
  return (
    <div className="flex h-full min-h-0 flex-col gap-4">
      <TrainerList />
      <ProcessList />
    </div>
  );
}
