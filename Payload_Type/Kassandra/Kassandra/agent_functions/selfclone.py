from mythic_container.MythicCommandBase import *
import json
from mythic_container.MythicRPC import *


class SelfCloneArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = []

    async def parse_arguments(self):
        if len(self.command_line.strip()) > 0:
            self.add_arg("parent", self.command_line.strip())
        else:
            self.add_arg("parent", "explorer.exe")


class SelfCloneCommand(CommandBase):
    cmd = "selfclone"
    needs_admin = False
    help_cmd = "selfclone [parent_process_name]"
    description = "Spawn a duplicate of the current process with a spoofed parent PID. The new process appears as a child of the specified parent (default: explorer.exe) in the process tree, breaking the real parent-child relationship."
    version = 1
    supported_ui_features = []
    author = "@PatchRequest"
    attackmapping = ["T1036.004"]
    argument_class = SelfCloneArguments
    attributes = CommandAttributes(
        builtin=False
    )

    async def create_go_tasking(self, taskData: MythicCommandBase.PTTaskMessageAllData) -> MythicCommandBase.PTTaskCreateTaskingMessageResponse:
        response = MythicCommandBase.PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )
        parent = taskData.args.get_arg("parent") or "explorer.exe"
        await SendMythicRPCArtifactCreate(MythicRPCArtifactCreateMessage(
            TaskID=taskData.Task.ID,
            ArtifactMessage=f"CreateProcessW with PPID spoof under {parent}",
            BaseArtifactType="API"
        ))
        return response

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        return resp
