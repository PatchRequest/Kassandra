from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *


class EmptyFilesystemArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            
        ]

    async def parse_arguments(self):
        pass

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)

class SingleFilesystemArguments(TaskArguments):
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

class MultiFilesystemArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="arg1",
                type=ParameterType.String,
                description="First argument (path or source path)",
            ),
            CommandParameter(
                name="arg2",
                type=ParameterType.String,
                description="Second argument (destination path, if needed)",
                default_value="",

                parameter_group_info=[
                    ParameterGroupInfo(group_name="Default", required=False),
                ]
            ),
        ]

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            raise ValueError("Must provide at least one path argument")
        parts = self.command_line.split()
        self.add_arg("arg1", parts[0])
        if len(parts) > 1:
            self.add_arg("arg2", parts[1])

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)


class LSCommand(CommandBase):
    cmd = "ls"
    needs_admin = False
    help_cmd = "ls {path}"
    description = "List files in directory"
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1083"]
    argument_class = SingleFilesystemArguments
    attributes = CommandAttributes(supported_os=[SupportedOS.Windows])

    async def create_tasking(self, task: MythicTask) -> MythicTask:
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)


class CDCommand(CommandBase):
    cmd = "cd"
    needs_admin = False
    help_cmd = "cd {path}"
    description = "Change working directory"
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1083"]
    argument_class = SingleFilesystemArguments
    attributes = CommandAttributes(supported_os=[SupportedOS.Windows])

    async def create_tasking(self, task: MythicTask) -> MythicTask:
        task.display_params = task.args.get_arg("arg1")
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)


class RMCommand(CommandBase):
    cmd = "rm"
    needs_admin = False
    help_cmd = "rm {path}"
    description = "Remove file or directory"
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1083"]
    argument_class = SingleFilesystemArguments
    attributes = CommandAttributes(supported_os=[SupportedOS.Windows])

    async def create_tasking(self, task: MythicTask) -> MythicTask:
        task.display_params = task.args.get_arg("arg1")
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)


class CPCommand(CommandBase):
    cmd = "cp"
    needs_admin = False
    help_cmd = "cp {source} {destination}"
    description = "Copy file"
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1083"]
    argument_class = MultiFilesystemArguments
    attributes = CommandAttributes(supported_os=[SupportedOS.Windows])

    async def create_tasking(self, task: MythicTask) -> MythicTask:
        task.display_params = f"{task.args.get_arg('arg1')} -> {task.args.get_arg('arg2')}"
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)


class MVCommand(CommandBase):
    cmd = "mv"
    needs_admin = False
    help_cmd = "mv {source} {destination}"
    description = "Move or rename file"
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1083"]
    argument_class = MultiFilesystemArguments
    attributes = CommandAttributes(supported_os=[SupportedOS.Windows])

    async def create_tasking(self, task: MythicTask) -> MythicTask:
        task.display_params = f"{task.args.get_arg('arg1')} -> {task.args.get_arg('arg2')}"
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)


class MKDIRCommand(CommandBase):
    cmd = "mkdir"
    needs_admin = False
    help_cmd = "mkdir {path}"
    description = "Create directory"
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1083"]
    argument_class = SingleFilesystemArguments
    attributes = CommandAttributes(supported_os=[SupportedOS.Windows])

    async def create_tasking(self, task: MythicTask) -> MythicTask:
        task.display_params = task.args.get_arg("arg1")
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)


class TOUCHCommand(CommandBase):
    cmd = "touch"
    needs_admin = False
    help_cmd = "touch {file}"
    description = "Create empty file"
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1083"]
    argument_class = SingleFilesystemArguments
    attributes = CommandAttributes(supported_os=[SupportedOS.Windows])

    async def create_tasking(self, task: MythicTask) -> MythicTask:
        task.display_params = task.args.get_arg("arg1")
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)

class PWDCommand(CommandBase):
    cmd = "pwd"
    needs_admin = False
    help_cmd = "pwd"
    description = "Print working directory"
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1083"]
    argument_class = EmptyFilesystemArguments
    attributes = CommandAttributes(supported_os=[SupportedOS.Windows])

    async def create_tasking(self, task: MythicTask) -> MythicTask:
        return task

    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)