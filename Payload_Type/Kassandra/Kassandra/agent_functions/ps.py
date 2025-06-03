from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
from mythic_container.PayloadBuilder import *


class SingleFilesystemArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="arg1",
                type=ParameterType.String,
                description="Command to execute",
            ),
        ]

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            raise ValueError("Must supply a text for the popup")
        self.add_arg("arg1", self.command_line)

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

class LSCommand(CommandBase):
    cmd = "psw"
    needs_admin = False
    help_cmd = "psw {command}"
    description = "Execute the command via powershell"
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1083"]
    argument_class = SingleFilesystemArguments
    attributes = CommandAttributes(supported_os=[SupportedOS.Windows])

    async def create_tasking(self, task: MythicTask) -> MythicTask:
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)