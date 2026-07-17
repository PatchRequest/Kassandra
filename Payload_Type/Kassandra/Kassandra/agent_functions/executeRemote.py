from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
import json, pathlib

CATALOG_ROOT = pathlib.Path("/opt/kassandra_catalog")
MANIFEST_PATH = CATALOG_ROOT / "manifest.json"


def _load_manifest():
    if not MANIFEST_PATH.exists():
        return []
    try:
        return json.loads(MANIFEST_PATH.read_text())
    except Exception:
        return []


async def populate_tool_names(callback: dict) -> list:
    entries = _load_manifest()
    # Group by type so operator scans quickly; filenames alphabetized inside each group
    bofs = sorted(m["name"] for m in entries if m.get("type") == "bof")
    nets = sorted(m["name"] for m in entries if m.get("type") == "dotnet")
    return bofs + nets


class ExecuteRemoteArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="tool_name",
                type=ParameterType.ChooseOne,
                description="Tool from the built-in catalog (tsec_*, outflank_*, sharp_*)",
                dynamic_query_function=populate_tool_names,
            ),
            CommandParameter(
                name="parameters",
                type=ParameterType.String,
                default_value="",
                description="BOF: 'str:foo wstr:bar int:5' | .NET: space-separated argv",
            ),
        ]

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            raise ValueError("Must supply arguments")
        raise ValueError("Must supply named arguments or use the modal")

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)


class ExecuteRemoteCommand(CommandBase):
    cmd = "executeRemote"
    needs_admin = False
    help_cmd = "executeRemote -tool_name <name> [-parameters <args>]"
    description = (
        "Run a BOF or .NET assembly from Kassandra's built-in catalog "
        "(TrustedSec CS-SA-BOF, Outflank C2-Tool-Collection, SharpCollection). "
        "Tool file is auto-resolved and shipped to the agent — no upload needed."
    )
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1132", "T1030", "T1105"]
    argument_class = ExecuteRemoteArguments

    async def create_go_tasking(self, taskData: MythicCommandBase.PTTaskMessageAllData) -> MythicCommandBase.PTTaskCreateTaskingMessageResponse:
        response = MythicCommandBase.PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )

        tool_name = taskData.args.get_arg("tool_name")
        params    = taskData.args.get_arg("parameters") or ""

        manifest = _load_manifest()
        entry = next((m for m in manifest if m.get("name") == tool_name), None)
        if entry is None:
            raise Exception(f"executeRemote: '{tool_name}' not found in catalog manifest")

        if entry["type"] == "bof":
            file_path = CATALOG_ROOT / "bof" / entry["filename"]
            target_command = "executeBOF"
        elif entry["type"] == "dotnet":
            file_path = CATALOG_ROOT / "dotnet" / entry["filename"]
            target_command = "executeDOT"
        else:
            raise Exception(f"executeRemote: unknown tool type '{entry.get('type')}'")

        if not file_path.exists():
            raise Exception(f"executeRemote: backing file missing on disk: {file_path}")

        file_resp = await SendMythicRPCFileCreate(MythicRPCFileCreateMessage(
            TaskID=taskData.Task.ID,
            FileContents=file_path.read_bytes(),
            Filename=file_path.name,
            DeleteAfterFetch=False,
            IsDownloadFromAgent=False,
        ))
        if not file_resp.Success:
            raise Exception(f"executeRemote: failed to register catalog file with Mythic: {file_resp.Error}")

        # Rewrite args to match the target command's schema (executeBOF / executeDOT both expect
        # {file_id, parameters}). Per Mythic SDK docs, modifying taskData.args is the supported
        # way to reshape what the agent sees — response.Params is explicitly discouraged.
        taskData.args.remove_arg("tool_name")
        taskData.args.add_arg("file_id", file_resp.AgentFileId)

        response.CommandName = target_command
        response.DisplayParams = f"-tool_name {tool_name}" + (f" -parameters \"{params}\"" if params else "")
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
