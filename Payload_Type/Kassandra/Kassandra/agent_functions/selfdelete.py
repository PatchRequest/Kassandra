from mythic_container.MythicCommandBase import *
import json
from mythic_container.MythicRPC import *


class SelfDeleteArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = []

    async def parse_arguments(self):
        pass


class SelfDeleteCommand(CommandBase):
    cmd = "selfdelete"
    needs_admin = False
    help_cmd = "selfdelete"
    description = "Delete the agent binary from disk using NTFS alternate data stream renaming. The process keeps running in memory."
    version = 1
    supported_ui_features = []
    author = "@PatchRequest"
    attackmapping = ["T1070.004"]
    argument_class = SelfDeleteArguments
    attributes = CommandAttributes(
        builtin=False
    )

    async def create_go_tasking(self, taskData: MythicCommandBase.PTTaskMessageAllData) -> MythicCommandBase.PTTaskCreateTaskingMessageResponse:
        response = MythicCommandBase.PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )
        await SendMythicRPCArtifactCreate(MythicRPCArtifactCreateMessage(
            TaskID=taskData.Task.ID,
            ArtifactMessage=f"SetFileInformationByHandle (FileRenameInfo + FileDispositionInfoEx)",
            BaseArtifactType="API"
        ))
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        return resp
