from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
from mythic_container.PayloadBuilder import *



class EmptyArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = []

    async def parse_arguments(self):
        pass

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

class ScreenshotCommand(CommandBase):
    cmd = "ping"
    needs_admin = False
    help_cmd = "ping pong"
    description = "It does Ping Pong"
    version = 1
    author = ""
    attackmapping = ["T1113"]
    argument_class = EmptyArguments
    attributes = CommandAttributes(
        spawn_and_injectable=True,
        supported_os=[SupportedOS.Windows],
        builtin=False,
        load_only=False,
        suggested_command=False,
    )
    script_only = False
    
    async def create_tasking(self, task: MythicTask) -> MythicTask:
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)