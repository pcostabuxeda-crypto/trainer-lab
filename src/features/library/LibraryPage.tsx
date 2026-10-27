import { Placeholder } from "../../components/ui/Placeholder";
import { ProcessList } from "./ProcessList";

export function LibraryPage() {
  return (
    <div className="flex h-full min-h-0 flex-col gap-4">
      <Placeholder title="Biblioteca de jocs" phase={4}>
        Aquí sortiran els jocs de Steam, Epic i GOG i els que afegeixis a mà. De moment pots veure els processos en
        execució.
      </Placeholder>
      <ProcessList />
    </div>
  );
}
