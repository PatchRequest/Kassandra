from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *


class EmptyArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            
        ]

    async def parse_arguments(self):
        pass

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

class SingleArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="arg1",
                type=ParameterType.String,
                description="First argument (path or source path)",
            ),
        ]

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            raise ValueError("Must supply a text for the popup")
        self.add_arg("arg1", self.command_line)

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)


class PivotStartCommand(CommandBase):
    cmd = "start_pivot"
    needs_admin = False
    help_cmd = "start_pivot {PORT}"
    description = "Start listening for pivot connections on the specified port"
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1083"]
    argument_class = SingleArguments
    attributes = CommandAttributes(supported_os=[SupportedOS.Windows])

    async def create_tasking(self, task: MythicTask) -> MythicTask:
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
    
class PivotStopCommand(CommandBase):
    cmd = "stop_pivot"
    needs_admin = False
    help_cmd = "stop_pivot {PORT}"
    description = "Stop listening for pivot connections on the specified port"
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1083"]
    argument_class = SingleArguments
    attributes = CommandAttributes(supported_os=[SupportedOS.Windows])

    async def create_tasking(self, task: MythicTask) -> MythicTask:
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
    
class PivotListCommand(CommandBase):
    cmd = "list_pivot"
    needs_admin = False
    help_cmd = "list_pivot"
    description = "List all active pivot ports"
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1083"]
    argument_class = EmptyArguments
    attributes = CommandAttributes(supported_os=[SupportedOS.Windows])

    async def create_tasking(self, task: MythicTask) -> MythicTask:
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)