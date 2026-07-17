from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
import json, pathlib

MANIFEST_PATH = pathlib.Path("/opt/kassandra_catalog/manifest.json")


class ListRemoteArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="filter",
                type=ParameterType.String,
                default_value="",
                description="Optional substring filter on tool name (e.g. 'kerb', 'sharp_')",
            ),
        ]

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            return
        self.add_arg("filter", self.command_line.strip())

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)


class ListRemoteCommand(CommandBase):
    cmd = "listRemote"
    needs_admin = False
    help_cmd = "listRemote [filter]"
    description = (
        "List tools available in Kassandra's built-in catalog (BOFs + .NET). "
        "Runs locally on the Mythic payload container; no agent round-trip."
    )
    version = 1
    author = "@PatchRequest"
    attackmapping = []
    argument_class = ListRemoteArguments

    async def create_go_tasking(self, taskData: MythicCommandBase.PTTaskMessageAllData) -> MythicCommandBase.PTTaskCreateTaskingMessageResponse:
        response = MythicCommandBase.PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
            Completed=True,
            TaskStatus="success",
        )

        if not MANIFEST_PATH.exists():
            response.Stdout = "Catalog manifest not found at /opt/kassandra_catalog/manifest.json"
            return response

        try:
            entries = json.loads(MANIFEST_PATH.read_text())
        except Exception as e:
            response.Stdout = f"Failed to parse manifest: {e}"
            return response

        flt = (taskData.args.get_arg("filter") or "").strip().lower()
        if flt:
            entries = [m for m in entries if flt in m.get("name", "").lower()]

        by_source_type = {}
        for m in entries:
            key = (m.get("source", "?"), m.get("type", "?"))
            by_source_type.setdefault(key, []).append(m)

        lines = []
        total = 0
        for (source, ttype), rows in sorted(by_source_type.items()):
            lines.append(f"=== {source} / {ttype} ({len(rows)}) ===")
            for m in sorted(rows, key=lambda x: x.get("name", "")):
                size_kb = m.get("size", 0) / 1024
                lines.append(f"  {m['name']:<45s} {size_kb:>7.1f} KB")
            lines.append("")
            total += len(rows)

        header = f"Kassandra catalog — {total} tool(s)"
        if flt:
            header += f" matching '{flt}'"
        lines.insert(0, header)
        lines.insert(1, "Usage: executeRemote -tool_name <name> [-parameters <args>]")
        lines.insert(2, "")

        response.DisplayParams = flt if flt else "(all)"
        output = "\n".join(lines)

        await SendMythicRPCResponseCreate(MythicRPCResponseCreateMessage(
            TaskID=taskData.Task.ID,
            Response=output.encode(),
        ))
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
