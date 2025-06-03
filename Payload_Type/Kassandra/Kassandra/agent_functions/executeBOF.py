from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
import json, sys, base64


class ExecuteBOFArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="file_id", 
                type=ParameterType.File, 
                description="file to upload"
            ),
            CommandParameter(
                name="parameters",
                type=ParameterType.String,
                description="--param=value",
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
    description = (
        "Executes a BOF "
    )
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1132", "T1030", "T1105"]
    argument_class = ExecuteBOFArguments


    async def create_go_tasking(self, taskData: MythicCommandBase.PTTaskMessageAllData) -> MythicCommandBase.PTTaskCreateTaskingMessageResponse:
        response = MythicCommandBase.PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )
        try:
            file_resp = await SendMythicRPCFileSearch(MythicRPCFileSearchMessage(
                TaskID=taskData.Task.ID,
                AgentFileID=taskData.args.get_arg("file")
            ))
            #taskData.args.get_arg("parameters")
                
        except Exception as e:
            raise Exception("Error from Mythic: " + str(sys.exc_info()[-1].tb_lineno) + " : " + str(e))
        return response


    async def process_response(self, task: PTTaskMessageAllData, response: any) -> PTTaskProcessResponseMessageResponse:
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        return resp