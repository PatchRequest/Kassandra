from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
import json, sys, base64


class ExecutePYArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="file_id",
                type=ParameterType.File,
                description="python script to upload",
            ),
            CommandParameter(
                name="python_embed_id",
                type=ParameterType.File,
                description="optional Python embeddable distribution zip",
                parameter_group_info=[ParameterGroupInfo(
                    required=False,
                    ui_position=2
                )],
            ),
            CommandParameter(
                name="parameters",
                type=ParameterType.String,
                description="script parameters",
            ),
        ]

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            raise ValueError("Must supply arguments")
        raise ValueError("Must supply named arguments or use the modal")

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)


class ExecutePYCommand(CommandBase):
    cmd = "executePY"
    needs_admin = False
    help_cmd = "executePY"
    description = "Executes a Python script"
    version = 1
    author = "@PatchRequest"
    attackmapping = ["T1132", "T1030", "T1105"]
    argument_class = ExecutePYArguments

    async def create_go_tasking(
        self, taskData: MythicCommandBase.PTTaskMessageAllData
    ) -> MythicCommandBase.PTTaskCreateTaskingMessageResponse:
        response = MythicCommandBase.PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )
        try:
            await SendMythicRPCFileSearch(
                MythicRPCFileSearchMessage(
                    TaskID=taskData.Task.ID,
                    AgentFileID=taskData.args.get_arg("file"),
                )
            )
            python_embed_id = taskData.args.get_arg("python_embed_id")
            if python_embed_id is not None and len(python_embed_id) > 0:
                await SendMythicRPCFileSearch(
                    MythicRPCFileSearchMessage(
                        TaskID=taskData.Task.ID,
                        AgentFileID=python_embed_id,
                    )
                )
        except Exception as e:
            raise Exception(
                "Error from Mythic: "
                + str(sys.exc_info()[-1].tb_lineno)
                + " : "
                + str(e)
            )
        return response

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        resp = PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
        return resp
