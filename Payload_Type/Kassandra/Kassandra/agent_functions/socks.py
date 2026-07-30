from mythic_container.MythicRPC import *
from mythic_container.MythicCommandBase import *


class SocksArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="port",
                cli_name="port",
                display_name="Port",
                type=ParameterType.Number,
                description="Port number on Mythic server for SOCKS5 (must be in MYTHIC_SERVER_DYNAMIC_PORTS)",
                parameter_group_info=[
                    ParameterGroupInfo(ui_position=0, required=True)
                ],
            ),
            CommandParameter(
                name="action",
                cli_name="action",
                display_name="Action",
                type=ParameterType.ChooseOne,
                choices=["start", "stop"],
                default_value="start",
                description="Start or stop the SOCKS5 proxy on the given port",
                parameter_group_info=[
                    ParameterGroupInfo(ui_position=1, required=False)
                ],
            ),
        ]

    async def parse_arguments(self):
        if len(self.command_line) == 0:
            raise Exception("Must be passed a port on the command line.")
        try:
            self.load_args_from_json_string(self.command_line)
        except Exception:
            port = self.command_line.lower().strip()
            try:
                self.add_arg("port", int(port))
            except Exception:
                raise Exception(
                    "Invalid port number given: {}. Must be int.".format(port)
                )

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)


class SocksCommand(CommandBase):
    cmd = "socks"
    needs_admin = False
    help_cmd = "socks <port>  |  socks {\"port\": 7000, \"action\": \"start|stop\"}"
    description = "Start or stop a SOCKS5 proxy on the Mythic server (tunneled through this callback)."
    version = 2
    author = "@checkymander"
    argument_class = SocksArguments
    attackmapping = ["T1090"]
    # Server-side only: Mythic opens the listener; agent handles socks[] on get_tasking.
    script_only = True
    attributes = CommandAttributes(
        load_only=False,
        builtin=False,
        supported_os=[SupportedOS.Windows],
    )

    async def create_go_tasking(
        self, taskData: PTTaskMessageAllData
    ) -> PTTaskCreateTaskingMessageResponse:
        response = PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )
        port = taskData.args.get_arg("port")
        action = taskData.args.get_arg("action") or "start"
        response.DisplayParams = f"action={action} port={port}"

        if action == "start":
            resp = await SendMythicRPCProxyStartCommand(
                MythicRPCProxyStartMessage(
                    TaskID=taskData.Task.ID,
                    PortType="socks",
                    LocalPort=port,
                )
            )
            if not resp.Success:
                response.TaskStatus = MythicStatus.Error
                response.Stderr = resp.Error
                response.Completed = True
                await SendMythicRPCResponseCreate(
                    MythicRPCResponseCreateMessage(
                        TaskID=taskData.Task.ID,
                        Response=resp.Error.encode(),
                    )
                )
            else:
                response.TaskStatus = MythicStatus.Success
                response.Completed = True
                await SendMythicRPCResponseCreate(
                    MythicRPCResponseCreateMessage(
                        TaskID=taskData.Task.ID,
                        Response=f"Started SOCKS5 server on port {port}".encode(),
                    )
                )
        else:
            resp = await SendMythicRPCProxyStopCommand(
                MythicRPCProxyStopMessage(
                    TaskID=taskData.Task.ID,
                    PortType="socks",
                    Port=port,
                )
            )
            if not resp.Success:
                response.TaskStatus = MythicStatus.Error
                response.Stderr = resp.Error
                response.Completed = True
                await SendMythicRPCResponseCreate(
                    MythicRPCResponseCreateMessage(
                        TaskID=taskData.Task.ID,
                        Response=resp.Error.encode(),
                    )
                )
            else:
                response.TaskStatus = MythicStatus.Success
                response.Completed = True
                await SendMythicRPCResponseCreate(
                    MythicRPCResponseCreateMessage(
                        TaskID=taskData.Task.ID,
                        Response=f"Stopped SOCKS5 server on port {port}".encode(),
                    )
                )
        return response

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
