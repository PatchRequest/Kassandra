from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
import json, sys, pathlib

BOF_LOADER_PATH = pathlib.Path("/opt/loaders/bof_loader.dll")


class ExecuteBOFArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="file_id",
                type=ParameterType.File,
                description="BOF file to execute",
            ),
            CommandParameter(
                name="parameters",
                type=ParameterType.String,
                description="Typed args: str:val wstr:val int:123 short:5 bin:b64data (no prefix defaults to str)",
            ),
        ]

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            raise ValueError("Must supply arguments")
        raise ValueError("Must supply named arguments or use the modal")

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)


class ExecuteBOFCommand(CommandBase):
    cmd = "executeBOF"
    needs_admin = False
    help_cmd = "executeBOF"
    description = "Execute a Beacon Object File via reflective in-memory loader"
    version = 2
    author = "@PatchRequest"
    attackmapping = ["T1132", "T1030", "T1105"]
    argument_class = ExecuteBOFArguments

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
        if BOF_LOADER_PATH.exists():
            loader_data = BOF_LOADER_PATH.read_bytes()
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
