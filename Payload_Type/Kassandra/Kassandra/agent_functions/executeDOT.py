from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
import json, sys, pathlib

DOT_LOADER_PATH = pathlib.Path("/opt/loaders/dot_loader.dll")


class ExecuteDOTArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="file_id",
                type=ParameterType.File,
                description=".NET assembly to execute",
            ),
            CommandParameter(
                name="parameters",
                type=ParameterType.String,
                description="Space-separated arguments for the .NET assembly",
            ),
        ]

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            raise ValueError("Must supply arguments")
        raise ValueError("Must supply named arguments or use the modal")

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)


class ExecuteDOTCommand(CommandBase):
    cmd = "executeDOT"
    needs_admin = False
    help_cmd = "executeDOT"
    description = "Execute a .NET assembly via reflective in-memory loader"
    version = 2
    author = "@PatchRequest"
    attackmapping = ["T1132", "T1030", "T1105"]
    argument_class = ExecuteDOTArguments

    async def create_go_tasking(self, taskData: MythicCommandBase.PTTaskMessageAllData) -> MythicCommandBase.PTTaskCreateTaskingMessageResponse:
        response = MythicCommandBase.PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )

        file_resp = await SendMythicRPCFileSearch(MythicRPCFileSearchMessage(
            TaskID=taskData.Task.ID,
            AgentFileID=taskData.args.get_arg("file_id"),
        ))

        loader_file_id = ""
        if DOT_LOADER_PATH.exists():
            loader_data = DOT_LOADER_PATH.read_bytes()
            create_resp = await SendMythicRPCFileCreate(MythicRPCFileCreateMessage(
                TaskID=taskData.Task.ID,
                FileContents=loader_data,
                DeleteAfterFetch=False,
            ))
            if create_resp.Success:
                loader_file_id = create_resp.AgentFileId

        taskData.args.add_arg("loader_file_id", loader_file_id, type=ParameterType.String)

        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
